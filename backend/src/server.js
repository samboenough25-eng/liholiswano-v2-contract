import 'dotenv/config';
import express from 'express';
import cors from 'cors';
import helmet from 'helmet';
import rateLimit from 'express-rate-limit';
import bcrypt from 'bcryptjs';
import jwt from 'jsonwebtoken';
import crypto from 'crypto';
import {Pool} from 'pg';
import {z} from 'zod';

const app=express();
const pool=new Pool({connectionString:process.env.DATABASE_URL,ssl:process.env.NODE_ENV==='production'?{rejectUnauthorized:false}:false});
app.use(helmet());
app.use(cors({origin:process.env.CORS_ORIGIN?.split(',').map(s=>s.trim())||true,credentials:true}));
app.use(express.json({limit:'256kb'}));
app.use(rateLimit({windowMs:900000,max:300,standardHeaders:true,legacyHeaders:false}));

const sign=u=>jwt.sign({sub:u.id,role:u.role},process.env.JWT_SECRET,{expiresIn:'30m'});
const auth=(req,res,next)=>{try{const h=req.headers.authorization||'';if(!h.startsWith('Bearer '))return res.status(401).json({error:'Authentication required'});req.user=jwt.verify(h.slice(7),process.env.JWT_SECRET);next()}catch{return res.status(401).json({error:'Invalid or expired session'})}};
const role=(...roles)=>(req,res,next)=>roles.includes(req.user.role)?next():res.status(403).json({error:'Insufficient permission'});
const audit=async(req,action,metadata={})=>{try{await pool.query('insert into audit_log(user_id,action,ip,metadata) values($1,$2,$3,$4)',[req.user?.sub||null,action,req.ip,metadata])}catch{}};
const kycVerified=async(req,res,next)=>{const r=await pool.query('select kyc_status from users where id=$1',[req.user.sub]);if(!r.rowCount)return res.status(401).json({error:'User not found'});if(r.rows[0].kyc_status!=='verified')return res.status(403).json({error:'KYC verification is required before this financial operation',kyc_status:r.rows[0].kyc_status});next()};

app.get('/health',async(_,res)=>{try{await pool.query('select 1');res.json({ok:true,service:'liholiswano-api',network:process.env.STELLAR_NETWORK||'testnet'})}catch{res.status(503).json({ok:false})}});

app.post('/api/auth/register',async(req,res)=>{
 const p=z.object({email:z.string().email(),password:z.string().min(12),fullName:z.string().min(2).max(120),country:z.enum(['BW','SZ'])}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid registration data'});
 const x=p.data;
 try{const hash=await bcrypt.hash(x.password,12);const r=await pool.query('insert into users(email,password_hash,full_name,country,email_verified) values($1,$2,$3,$4,false) returning id,email,full_name,country,role,kyc_status,email_verified',[x.email.toLowerCase(),hash,x.fullName,x.country]);res.status(201).json({user:r.rows[0],token:sign(r.rows[0]),message:'Account created. Email verification is the next required step.'})}
 catch{res.status(409).json({error:'Email is already registered'})}
});

app.post('/api/auth/login',async(req,res)=>{
 const p=z.object({email:z.string().email(),password:z.string()}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid login'});
 const r=await pool.query('select * from users where email=$1',[p.data.email.toLowerCase()]);
 if(!r.rowCount||!(await bcrypt.compare(p.data.password,r.rows[0].password_hash)))return res.status(401).json({error:'Invalid email or password'});
 const u=r.rows[0];delete u.password_hash;res.json({user:u,token:sign(u),portal:['owner','admin'].includes(u.role)?'owner':u.role==='compliance'?'compliance':'customer'});
});

app.get('/api/me',auth,async(req,res)=>{const r=await pool.query('select id,email,full_name,country,role,kyc_status,kyc_reference,created_at from users where id=$1',[req.user.sub]);res.json(r.rows[0]||null)});

app.post('/api/kyc/session',auth,async(req,res)=>{
 const ref='KYC-'+crypto.randomUUID();await pool.query('update users set kyc_reference=$1,updated_at=now() where id=$2',[ref,req.user.sub]);await audit(req,'kyc.session.created',{provider:process.env.KYC_PROVIDER||'stub'});res.status(201).json({status:'pending',reference:ref,provider:process.env.KYC_PROVIDER||'stub',message:'Connect an approved KYC provider before production activation.'})
});

app.get('/api/transactions',auth,async(req,res)=>{const r=await pool.query('select id,group_id,type,asset,amount,stellar_hash,status,metadata,created_at from transactions where user_id=$1 order by created_at desc limit 100',[req.user.sub]);res.json(r.rows)});
app.get('/api/notifications',auth,async(req,res)=>{const r=await pool.query('select * from notifications where user_id=$1 order by created_at desc limit 100',[req.user.sub]);res.json(r.rows)});
app.get('/api/groups',auth,async(req,res)=>{const r=await pool.query(`select g.id,g.contract_id,g.status,g.created_at,m.status as membership_status,m.joined_at from groups g join memberships m on m.group_id=g.id where m.user_id=$1 order by g.created_at desc`,[req.user.sub]);res.json(r.rows)});
app.post('/api/wallets',auth,async(req,res)=>{const p=z.object({network:z.enum(['testnet','mainnet']),publicKey:z.string().min(50).max(60)}).safeParse(req.body);if(!p.success)return res.status(400).json({error:'Invalid wallet'});const r=await pool.query('insert into wallets(user_id,network,public_key) values($1,$2,$3) on conflict(user_id,network) do update set public_key=excluded.public_key returning id,network,public_key',[req.user.sub,p.data.network,p.data.publicKey]);await audit(req,'wallet.linked',{network:p.data.network});res.status(201).json(r.rows[0])});

app.get('/api/admin/stats',auth,role('owner','admin'),async(_,res)=>{
 const r=await pool.query(`select
 (select count(*) from users where role='customer') customers,
 (select count(*) from users where kyc_status='pending') pending_kyc,
 (select count(*) from groups) groups,
 (select count(*) from transactions) transactions,
 (select coalesce(sum(amount),0) from transactions where status='confirmed') confirmed_volume`);
 res.json(r.rows[0]);
});
app.get('/api/admin/users',auth,role('owner','admin','compliance'),async(req,res)=>{
 const r=await pool.query('select id,email,full_name,country,role,kyc_status,created_at from users order by created_at desc limit 500');res.json(r.rows);
});
app.get('/api/admin/kyc',auth,role('owner','compliance'),async(req,res)=>{
 const r=await pool.query("select id,email,full_name,country,kyc_status,kyc_reference,created_at from users where role='customer' order by case when kyc_status='pending' then 0 else 1 end,created_at desc limit 500");res.json(r.rows);
});
app.get('/api/admin/transactions',auth,role('owner','admin'),async(req,res)=>{
 const r=await pool.query(`select t.id,t.type,t.asset,t.amount,t.stellar_hash,t.status,t.created_at,u.email,u.full_name,g.id as group_id from transactions t left join users u on u.id=t.user_id left join groups g on g.id=t.group_id order by t.created_at desc limit 500`);res.json(r.rows);
});
app.get('/api/admin/audit',auth,role('owner'),async(req,res)=>{const r=await pool.query(`select a.id,a.action,a.ip,a.metadata,a.created_at,u.email from audit_log a left join users u on u.id=a.user_id order by a.created_at desc limit 500`);res.json(r.rows)});
app.patch('/api/admin/users/:id/kyc',auth,role('owner','compliance'),async(req,res)=>{
 const p=z.object({status:z.enum(['pending','verified','rejected'])}).safeParse(req.body);if(!p.success)return res.status(400).json({error:'Invalid KYC status'});
 await pool.query('update users set kyc_status=$1,updated_at=now() where id=$2',[p.data.status,req.params.id]);await audit(req,'kyc.status.changed',{target:req.params.id,status:p.data.status});res.json({ok:true});
});
app.use((err,_,res,next)=>{console.error(err);if(res.headersSent)return next(err);res.status(500).json({error:'Internal server error'})});
app.listen(Number(process.env.PORT||8080),()=>console.log('Liholiswano API started'));
