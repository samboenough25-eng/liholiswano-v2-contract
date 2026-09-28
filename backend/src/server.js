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
import {createHostedKycSession,confirmCallbackSignature,normalizeKycDecision,kycConfigured} from './smile-id.js';
import {execFile} from 'child_process';
import {promisify} from 'util';
const execFileAsync=promisify(execFile);

const app=express();
app.disable('x-powered-by');
app.set('trust proxy',1);
if(process.env.NODE_ENV==='production' && !process.env.JWT_SECRET) throw new Error('JWT_SECRET is required in production');
if(!process.env.DATABASE_URL) throw new Error('DATABASE_URL is required');
const STELLAR_NETWORK=process.env.STELLAR_NETWORK||'testnet';
const STELLAR_RPC_URL=process.env.STELLAR_RPC_URL||'https://soroban-testnet.stellar.org';
const pool=new Pool({connectionString:process.env.DATABASE_URL,ssl:process.env.NODE_ENV==='production'?{rejectUnauthorized:false}:false});
app.use(helmet());
const allowedOrigins=[...(process.env.CORS_ORIGIN||'').split(',').map(s=>s.trim()).filter(Boolean),'https://liholiswano-web.onrender.com','https://samboenough25-eng.github.io'].filter((v,i,a)=>a.indexOf(v)===i);
if(process.env.NODE_ENV==='production' && !allowedOrigins.length) throw new Error('CORS_ORIGIN is required in production');
if(process.env.NODE_ENV==='production' && STELLAR_NETWORK==='mainnet'){
 const required=[['RESEND_API_KEY',process.env.RESEND_API_KEY],['EMAIL_FROM',process.env.EMAIL_FROM],['SMS_PROVIDER_URL',process.env.SMS_PROVIDER_URL],['SMS_PROVIDER_API_KEY',process.env.SMS_PROVIDER_API_KEY],['RECONCILE_SECRET',process.env.RECONCILE_SECRET]];
 const missing=required.filter(([,v])=>!v).map(([k])=>k);
 if((process.env.KYC_PROVIDER||'stub').toLowerCase()==='stub') missing.push('KYC_PROVIDER(non-stub)');
 if((process.env.KYC_PROVIDER||'stub').toLowerCase()==='smile'){if(!process.env.SMILE_PARTNER_ID) missing.push('SMILE_PARTNER_ID');if(!process.env.SMILE_API_KEY) missing.push('SMILE_API_KEY');if(!process.env.KYC_CALLBACK_URL) missing.push('KYC_CALLBACK_URL');}
 if(missing.length) throw new Error('Mainnet production configuration incomplete: '+missing.join(', '));
}
app.use(cors({origin:(origin,cb)=>{if(!origin||allowedOrigins.includes(origin))return cb(null,true);cb(new Error('Origin not allowed'));},credentials:true}));
app.use(express.json({limit:'256kb'}));
app.use(rateLimit({windowMs:900000,max:300,standardHeaders:true,legacyHeaders:false}));
const authLimiter=rateLimit({windowMs:900000,max:10,standardHeaders:true,legacyHeaders:false,message:{error:'Too many authentication attempts. Try again later.'}});
const codeLimiter=rateLimit({windowMs:900000,max:5,standardHeaders:true,legacyHeaders:false,message:{error:'Too many verification attempts. Try again later.'}});

const sign=u=>jwt.sign({sub:u.id,role:u.role},process.env.JWT_SECRET,{expiresIn:'30m'});
const auth=(req,res,next)=>{try{const h=req.headers.authorization||'';if(!h.startsWith('Bearer '))return res.status(401).json({error:'Authentication required'});req.user=jwt.verify(h.slice(7),process.env.JWT_SECRET);next()}catch{return res.status(401).json({error:'Invalid or expired session'})}};
const role=(...roles)=>(req,res,next)=>roles.includes(req.user.role)?next():res.status(403).json({error:'Insufficient permission'});
const audit=async(req,action,metadata={})=>{try{await pool.query('insert into audit_log(user_id,action,ip,metadata) values($1,$2,$3,$4)',[req.user?.sub||null,action,req.ip,metadata])}catch{}};
const kycVerified=async(req,res,next)=>{const r=await pool.query('select kyc_status from users where id=$1',[req.user.sub]);if(!r.rowCount)return res.status(401).json({error:'User not found'});if(r.rows[0].kyc_status!=='verified')return res.status(403).json({error:'KYC verification is required before this financial operation',kyc_status:r.rows[0].kyc_status});next()};
const verifiedAccount=async(req,res,next)=>{const r=await pool.query('select email_verified from users where id=$1',[req.user.sub]);if(!r.rowCount)return res.status(401).json({error:'User not found'});if(!r.rows[0].email_verified)return res.status(403).json({error:'Email verification is required',email_verification_required:true});next()};
const hashToken=v=>crypto.createHash('sha256').update(v).digest('hex');
const makeCode=()=>String(crypto.randomInt(100000,1000000));
async function stellarTransactionStatus(hash){const r=await fetch(STELLAR_RPC_URL,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:Date.now(),method:'getTransaction',params:{hash}})});if(!r.ok)throw new Error('Stellar RPC HTTP '+r.status);const j=await r.json();if(j.error)throw new Error(j.error.message||'Stellar RPC error');return j.result;}
const sendVerificationEmail=async(to,code)=>{
 const key=process.env.RESEND_API_KEY;
 const from=process.env.EMAIL_FROM;
 if(!key||!from){
   if(process.env.NODE_ENV==='production')throw new Error('Email provider is not configured');
   console.log(`DEV EMAIL VERIFICATION for ${to}: ${code}`);
   return {delivered:false,development:true};
 }
 const r=await fetch('https://api.resend.com/emails',{method:'POST',headers:{Authorization:`Bearer ${key}`,'Content-Type':'application/json'},body:JSON.stringify({from,to,subject:'Verify your Liholiswano email',text:`Your Liholiswano verification code is ${code}. It expires in 15 minutes.`})});
 if(!r.ok)throw new Error('Email delivery failed');
 return {delivered:true};
};

app.get('/health',async(_,res)=>{try{await pool.query('select 1');res.json({ok:true,service:'liholiswano-api',network:process.env.STELLAR_NETWORK||'testnet'})}catch{res.status(503).json({ok:false})}});

app.post('/internal/reconcile',async(req,res)=>{
 const expected=process.env.RECONCILE_SECRET;
 if(!expected||req.get('x-reconcile-secret')!==expected)return res.status(401).json({error:'Unauthorized'});
 try{
   const {stdout,stderr}=await execFileAsync(process.execPath,['src/reconcile.js'],{cwd:new URL('..',import.meta.url).pathname,timeout:120000,maxBuffer:1024*1024});
   res.json({ok:true,output:stdout.trim(),error:stderr.trim()||undefined});
 }catch(e){console.error('reconcile endpoint',e);res.status(500).json({ok:false,error:'Reconciliation failed'});}
});

app.post('/api/auth/register',authLimiter,async(req,res)=>{
 const p=z.object({email:z.string().email(),password:z.string().min(12),fullName:z.string().min(2).max(120),country:z.enum(['BW','SZ'])}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid registration data'});
 const x=p.data;
 try{
   const hash=await bcrypt.hash(x.password,12);
   const r=await pool.query('insert into users(email,password_hash,full_name,country,email_verified) values($1,$2,$3,$4,false) returning id,email,full_name,country,role,kyc_status,email_verified',[x.email.toLowerCase(),hash,x.fullName,x.country]);
   const code=makeCode();
   await pool.query("insert into verification_tokens(user_id,channel,token_hash,expires_at) values($1,'email',$2,now()+interval '15 minutes')",[r.rows[0].id,hashToken(code)]);
   let delivery;
   try{delivery=await sendVerificationEmail(r.rows[0].email,code)}
   catch{await pool.query('delete from verification_tokens where user_id=$1 and channel=\'email\' and used_at is null',[r.rows[0].id]);await pool.query('delete from users where id=$1',[r.rows[0].id]);return res.status(503).json({error:'Email delivery is not configured yet. Connect the email provider before creating accounts.'})}
   res.status(201).json({user:r.rows[0],token:sign(r.rows[0]),email_verification_required:true,development_code:delivery.development?code:undefined,message:'Account created. Verify your email before using financial features.'})
 }catch(e){if(e?.code==='23505')return res.status(409).json({error:'Email is already registered'});console.error(e);res.status(500).json({error:'Could not create account'})}
});

app.post('/api/auth/resend-verification',auth,codeLimiter,async(req,res)=>{
 const r=await pool.query('select id,email,email_verified from users where id=$1',[req.user.sub]);
 if(!r.rowCount)return res.status(404).json({error:'User not found'});
 if(r.rows[0].email_verified)return res.json({verified:true,message:'Email is already verified'});
 await pool.query("update verification_tokens set used_at=now() where user_id=$1 and channel='email' and used_at is null",[req.user.sub]);
 const code=makeCode();
 await pool.query("insert into verification_tokens(user_id,channel,token_hash,expires_at) values($1,'email',$2,now()+interval '15 minutes')",[req.user.sub,hashToken(code)]);
 try{
   const delivery=await sendVerificationEmail(r.rows[0].email,code);
   res.json({verified:false,development_code:delivery.development?code:undefined,message:'A new verification code was sent.'});
 }catch(e){res.status(503).json({error:'Email delivery is not configured yet'});}
});
app.post('/api/auth/set-phone',auth,verifiedAccount,async(req,res)=>{
 const p=z.object({phone:z.string().regex(/^\+?[1-9]\d{7,14}$/)}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Enter a valid international phone number'});
 await pool.query('update users set phone=$1,phone_verified=false,updated_at=now() where id=$2',[p.data.phone,req.user.sub]);
 await pool.query("update verification_tokens set used_at=now() where user_id=$1 and channel='phone' and used_at is null",[req.user.sub]);
 const code=makeCode();
 await pool.query("insert into verification_tokens(user_id,channel,token_hash,expires_at) values($1,'phone',$2,now()+interval '15 minutes')",[req.user.sub,hashToken(code)]);
 const url=process.env.SMS_PROVIDER_URL,key=process.env.SMS_PROVIDER_API_KEY;
 if(!url||!key)return res.status(503).json({error:'SMS provider is not configured yet'});
 const rr=await fetch(url,{method:'POST',headers:{Authorization:'Bearer '+key,'Content-Type':'application/json'},body:JSON.stringify({to:p.data.phone,message:'Your Liholiswano phone verification code is '+code+'. It expires in 15 minutes.'})});
 if(!rr.ok)return res.status(503).json({error:'SMS delivery failed'});
 res.json({message:'Verification code sent.'});
});
app.post('/api/auth/verify-phone',auth,verifiedAccount,codeLimiter,async(req,res)=>{
 const p=z.object({code:z.string().regex(/^\d{6}$/)}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Enter a valid 6-digit code'});
 const t=await pool.query("select id from verification_tokens where user_id=$1 and channel='phone' and token_hash=$2 and used_at is null and expires_at>now() order by created_at desc limit 1",[req.user.sub,hashToken(p.data.code)]);
 if(!t.rowCount)return res.status(400).json({error:'Invalid or expired phone verification code'});
 await pool.query('update verification_tokens set used_at=now() where id=$1',[t.rows[0].id]);
 await pool.query('update users set phone_verified=true,updated_at=now() where id=$1',[req.user.sub]);
 await audit(req,'auth.phone.verified',{});
 res.json({verified:true,message:'Phone verified successfully.'});
});
app.post('/api/auth/verify-email',codeLimiter,async(req,res)=>{
 const p=z.object({email:z.string().email(),code:z.string().regex(/^\d{6}$/)}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Enter a valid email and 6-digit code'});
 const u=await pool.query('select id,email,email_verified from users where email=$1',[p.data.email.toLowerCase()]);
 if(!u.rowCount)return res.status(400).json({error:'Invalid verification code'});
 if(u.rows[0].email_verified)return res.json({verified:true,message:'Email is already verified'});
 const t=await pool.query("select id from verification_tokens where user_id=$1 and channel='email' and token_hash=$2 and used_at is null and expires_at>now() order by created_at desc limit 1",[u.rows[0].id,hashToken(p.data.code)]);
 if(!t.rowCount)return res.status(400).json({error:'Invalid or expired verification code'});
 await pool.query('update verification_tokens set used_at=now() where id=$1',[t.rows[0].id]);
 await pool.query('update users set email_verified=true,updated_at=now() where id=$1',[u.rows[0].id]);
 await pool.query('insert into audit_log(user_id,action,metadata) values($1,$2,$3)',[u.rows[0].id,'auth.email.verified',{}]);
 res.json({verified:true,message:'Email verified successfully. You can now sign in.'});
});
app.post('/api/auth/forgot-password',authLimiter,async(req,res)=>{
 const p=z.object({email:z.string().email()}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Enter a valid email'});
 const r=await pool.query('select id,email from users where email=$1',[p.data.email.toLowerCase()]);
 if(!r.rowCount)return res.json({message:'If that email exists, a reset message will be sent.'});
 const raw=crypto.randomBytes(32).toString('hex');
 await pool.query("update password_reset_tokens set used_at=now() where user_id=$1 and used_at is null",[r.rows[0].id]);
 await pool.query("insert into password_reset_tokens(user_id,token_hash,expires_at) values($1,$2,now()+interval '30 minutes')",[r.rows[0].id,hashToken(raw)]);
 const key=process.env.RESEND_API_KEY,from=process.env.EMAIL_FROM;
 if(!key||!from)return res.status(503).json({error:'Password recovery email is not configured yet'});
 const rr=await fetch('https://api.resend.com/emails',{method:'POST',headers:{Authorization:'Bearer '+key,'Content-Type':'application/json'},body:JSON.stringify({from,to:r.rows[0].email,subject:'Reset your Liholiswano password',text:'Use this password reset token within 30 minutes: '+raw})});
 if(!rr.ok)return res.status(503).json({error:'Password recovery email could not be sent'});
 res.json({message:'If that email exists, a reset message will be sent.'});
});
app.post('/api/auth/reset-password',authLimiter,async(req,res)=>{
 const p=z.object({email:z.string().email(),token:z.string().min(40).max(100),password:z.string().min(12)}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid password reset request'});
 const u=await pool.query('select id from users where email=$1',[p.data.email.toLowerCase()]);
 if(!u.rowCount)return res.status(400).json({error:'Invalid or expired reset token'});
 const t=await pool.query("select id from password_reset_tokens where user_id=$1 and token_hash=$2 and used_at is null and expires_at>now() order by created_at desc limit 1",[u.rows[0].id,hashToken(p.data.token)]);
 if(!t.rowCount)return res.status(400).json({error:'Invalid or expired reset token'});
 const hash=await bcrypt.hash(p.data.password,12);
 await pool.query('update users set password_hash=$1,updated_at=now() where id=$2',[hash,u.rows[0].id]);
 await pool.query('update password_reset_tokens set used_at=now() where user_id=$1 and used_at is null',[u.rows[0].id]);
 await pool.query('insert into audit_log(user_id,action,metadata) values($1,$2,$3)',[u.rows[0].id,'auth.password.reset',{}]);
 res.json({message:'Password reset successfully. Please sign in again.'});
});
app.post('/api/auth/login',authLimiter,async(req,res)=>{
 const p=z.object({email:z.string().email(),password:z.string()}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid login'});
 const r=await pool.query('select * from users where email=$1',[p.data.email.toLowerCase()]);
 if(!r.rowCount||!(await bcrypt.compare(p.data.password,r.rows[0].password_hash)))return res.status(401).json({error:'Invalid email or password'});
 const u=r.rows[0];delete u.password_hash;res.json({user:u,token:sign(u),portal:['owner','admin'].includes(u.role)?'owner':u.role==='compliance'?'compliance':'customer',email_verification_required:u.email_verified===false});
});

app.get('/api/me',auth,async(req,res)=>{const r=await pool.query('select id,email,full_name,country,role,kyc_status,kyc_reference,created_at from users where id=$1',[req.user.sub]);res.json(r.rows[0]||null)});

app.post('/api/kyc/session',auth,verifiedAccount,async(req,res)=>{
 const p=z.object({documentType:z.enum(['BW_OMANG','BW_PASSPORT','SZ_ID','SZ_PASSPORT'])}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Select a valid identity document type'});
 const provider=(process.env.KYC_PROVIDER||'stub').toLowerCase();
 const ref='KYC-'+crypto.randomUUID();
 try{
   if(provider==='stub'){
     const r=await pool.query("insert into kyc_sessions(user_id,provider,reference,status,document_type) values($1,$2,$3,'pending',$4) returning id,reference,status,document_type,created_at",[req.user.sub,provider,ref,p.data.documentType]);
     await pool.query("update users set kyc_reference=$1,kyc_status='in_progress',updated_at=now() where id=$2",[ref,req.user.sub]);
     await audit(req,'kyc.session.created',{provider,document_type:p.data.documentType});
     return res.status(201).json({status:'pending',reference:ref,session_id:r.rows[0].id,document_type:p.data.documentType,provider,message:'KYC provider is still in stub mode. Connect an approved provider before production activation.'});
   }
   if(provider!=='smile') return res.status(503).json({error:'Unsupported KYC provider'});
   if(!kycConfigured()) return res.status(503).json({error:'Smile ID KYC is not configured on the API yet'});
   const r=await pool.query("insert into kyc_sessions(user_id,provider,reference,status,document_type,result) values($1,'smile',$2,'pending',$3,$4) returning id,reference,status,document_type,created_at",[req.user.sub,ref,p.data.documentType,JSON.stringify({document_type:p.data.documentType})]);
   const token=await createHostedKycSession({userId:req.user.sub,jobId:ref,product:process.env.SMILE_KYC_PRODUCT||'enhanced_kyc'});
   await pool.query("update kyc_sessions set result=result||$1::jsonb,updated_at=now() where id=$2",[JSON.stringify({provider:'smile',job_id:ref}),r.rows[0].id]);
   await pool.query("update users set kyc_reference=$1,kyc_status='in_progress',updated_at=now() where id=$2",[ref,req.user.sub]);
   await audit(req,'kyc.session.created',{provider:'smile',document_type:p.data.documentType,job_id:ref});
   return res.status(201).json({status:'pending',reference:ref,session_id:r.rows[0].id,document_type:p.data.documentType,provider:'smile',product:process.env.SMILE_KYC_PRODUCT||'enhanced_kyc',token:token.token||token});
 }catch(e){console.error('KYC session creation failed',e);return res.status(502).json({error:'Could not start identity verification with the KYC provider'});}
});

app.post('/api/kyc/callback',async(req,res)=>{
 try{
   const payload=req.body||{};
   const timestamp=payload.timestamp||req.get('x-smile-timestamp');
   const signature=payload.signature||req.get('x-smile-signature');
   if(!confirmCallbackSignature(timestamp,signature)) return res.status(401).json({error:'Invalid KYC callback signature'});
   const partner=payload.PartnerParams||payload.partner_params||{};
   const jobId=partner.job_id||payload.job_id||payload.JobID;
   if(!jobId)return res.status(400).json({error:'KYC callback is missing job_id'});
   const d=normalizeKycDecision(payload);
   const status=d.status==='verified'?'verified':d.status==='rejected'?'rejected':'pending';
   const session=await pool.query("select id,user_id from kyc_sessions where provider='smile' and reference=$1 order by created_at desc limit 1",[jobId]);
   if(!session.rowCount)return res.status(404).json({error:'KYC session not found'});
   await pool.query("update kyc_sessions set status=$1,document_verified=$2,face_verified=$3,liveness_verified=$4,aml_screened=$5,pep_screened=$6,duplicate_face_checked=$7,result=result||$8::jsonb,updated_at=now() where id=$9",[status,status==='verified',status==='verified',status==='verified',status==='verified',false,false,JSON.stringify({result_code:d.resultCode,result_text:d.resultText,final:d.final,smile_job_id:d.smileJobId,callback_received_at:new Date().toISOString()}),session.rows[0].id]);
   if(status==='verified') await pool.query("update users set kyc_status='verified',updated_at=now() where id=$1",[session.rows[0].user_id]);
   else if(status==='rejected') await pool.query("update users set kyc_status='rejected',updated_at=now() where id=$1",[session.rows[0].user_id]);
   await pool.query("insert into audit_log(user_id,action,metadata) values($1,$2,$3)",[session.rows[0].user_id,'kyc.provider.callback',{provider:'smile',job_id:jobId,status,result_code:d.resultCode}]);
   return res.json({ok:true,status});
 }catch(e){console.error('KYC callback failed',e);return res.status(500).json({error:'KYC callback processing failed'});}
});

app.get('/api/kyc/status',auth,verifiedAccount,async(req,res)=>{
 const r=await pool.query("select u.kyc_status,u.kyc_reference,k.provider,k.status,k.document_type,k.document_verified,k.face_verified,k.liveness_verified,k.aml_screened,k.pep_screened,k.duplicate_face_checked,k.created_at,k.updated_at from users u left join lateral (select * from kyc_sessions where user_id=u.id order by created_at desc limit 1) k on true where u.id=$1",[req.user.sub]);
 res.json(r.rows[0]||null);
});

app.get('/api/transactions',auth,verifiedAccount,async(req,res)=>{const r=await pool.query('select id,group_id,type,asset,amount,stellar_hash,status,metadata,created_at from transactions where user_id=$1 order by created_at desc limit 100',[req.user.sub]);res.json(r.rows)});
app.post('/api/transactions',auth,verifiedAccount,role('customer','owner','admin'),async(req,res)=>{
 const p=z.object({groupId:z.string().min(1).max(32).optional(),type:z.enum(['join','contribute','bid','settle','refund','default','create_group','lock_group']),asset:z.string().max(80).optional(),amount:z.number().finite().nonnegative().optional(),stellarHash:z.string().regex(/^[a-f0-9]{64}$/i),metadata:z.record(z.any()).default({})}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid transaction record'});
 const existing=await pool.query('select id,status from transactions where stellar_hash=$1 limit 1',[p.data.stellarHash]);
 if(existing.rowCount)return res.status(409).json({error:'This Stellar transaction has already been recorded',transaction_id:existing.rows[0].id,status:existing.rows[0].status});
 let chainStatus='pending';
 try{const chain=await stellarTransactionStatus(p.data.stellarHash);if(chain.status==='SUCCESS')chainStatus='confirmed';else if(chain.status==='FAILED')chainStatus='failed';}catch(e){return res.status(503).json({error:'Could not verify the Stellar transaction yet. Try again shortly.'});}
 let r;
 try{ r=await pool.query('insert into transactions(user_id,group_id,type,asset,amount,stellar_hash,status,metadata) values($1,$2,$3,$4,$5,$6,$7,$8) returning id,group_id,type,asset,amount,stellar_hash,status,created_at',[req.user.sub,p.data.groupId||null,p.data.type,p.data.asset||null,p.data.amount??null,p.data.stellarHash,chainStatus,p.data.metadata]);}catch(e){if(e?.code==='23505')return res.status(409).json({error:'This Stellar transaction has already been recorded'});throw e;}
 await audit(req,'transaction.recorded',{type:p.data.type,stellar_hash:p.data.stellarHash,group_id:p.data.groupId||null,status:chainStatus});
 res.status(chainStatus==='confirmed'?201:202).json(r.rows[0]);
});

app.get('/api/notifications',auth,verifiedAccount,async(req,res)=>{const r=await pool.query('select * from notifications where user_id=$1 order by created_at desc limit 100',[req.user.sub]);res.json(r.rows)});
app.get('/api/groups',auth,verifiedAccount,async(req,res)=>{const r=await pool.query(`select g.id,g.contract_id,g.status,g.created_at,m.status as membership_status,m.joined_at from groups g join memberships m on m.group_id=g.id where m.user_id=$1 order by g.created_at desc`,[req.user.sub]);res.json(r.rows)});
app.post('/api/groups/register',auth,verifiedAccount,role('owner','admin'),async(req,res)=>{
 const p=z.object({id:z.string().min(1).max(32).regex(/^[A-Za-z0-9_]+$/),contractId:z.string().regex(/^C[A-Z2-7]{55}$/),status:z.enum(['open','locked','completed']).default('open')}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid group registration'});
 const r=await pool.query('insert into groups(id,contract_id,admin_user_id,status) values($1,$2,$3,$4) on conflict(id) do update set contract_id=excluded.contract_id,status=excluded.status returning *',[p.data.id,p.data.contractId,req.user.sub,p.data.status]);
 await pool.query("insert into memberships(group_id,user_id,status) values($1,$2,'active') on conflict(group_id,user_id) do update set status='active'",[p.data.id,req.user.sub]);
 await audit(req,'group.registered',{group_id:p.data.id,contract_id:p.data.contractId});
 res.status(201).json(r.rows[0]);
});

app.post('/api/groups/:id/join',auth,verifiedAccount,kycVerified,async(req,res)=>{
 const id=req.params.id;
 const g=await pool.query('select id,status from groups where id=$1',[id]);
 if(!g.rowCount)return res.status(404).json({error:'Group is not registered in the platform'});
 if(g.rows[0].status==='completed')return res.status(409).json({error:'Group is already completed'});
 const r=await pool.query("insert into memberships(group_id,user_id,status) values($1,$2,'active') on conflict(group_id,user_id) do update set status='active' returning group_id,user_id,status,joined_at",[id,req.user.sub]);
 await audit(req,'group.member.joined',{group_id:id});
 res.status(201).json(r.rows[0]);
});

app.post('/api/wallets',auth,verifiedAccount,kycVerified,async(req,res)=>{const p=z.object({network:z.enum(['testnet','mainnet']),publicKey:z.string().regex(/^G[A-Z2-7]{55}$/)}).safeParse(req.body);if(!p.success)return res.status(400).json({error:'Invalid Stellar wallet address'});if(p.data.network!==STELLAR_NETWORK)return res.status(400).json({error:'Wallet network does not match the active platform network',network:STELLAR_NETWORK});const r=await pool.query('insert into wallets(user_id,network,public_key) values($1,$2,$3) on conflict(user_id,network) do update set public_key=excluded.public_key returning id,network,public_key',[req.user.sub,p.data.network,p.data.publicKey]);await audit(req,'wallet.linked',{network:p.data.network});res.status(201).json(r.rows[0])});


// ── Treasury / customer funding ────────────────────────────────────────
// The owner funds a customer's wallet from an externally controlled treasury
// wallet. The API never holds a private key and never signs a payment.
// Flow: create order -> owner sends the payment with their wallet -> confirm
// with the Stellar transaction hash -> API verifies the on-chain result.
app.get('/api/assets',auth,verifiedAccount,async(_,res)=>{
 const r=await pool.query("select id,symbol,issuer,network,decimals,contract_address,status from supported_assets where status='active' order by symbol");
 res.json(r.rows);
});

app.get('/api/funding-orders',auth,verifiedAccount,async(req,res)=>{
 const r=await pool.query(`select f.id,f.amount,f.stellar_hash,f.status,f.created_at,f.confirmed_at,
   a.symbol,a.issuer,a.network,a.decimals,a.contract_address
   from funding_orders f join supported_assets a on a.id=f.asset_id
   where f.user_id=$1 order by f.created_at desc limit 100`,[req.user.sub]);
 res.json(r.rows);
});

app.get('/api/admin/treasury',auth,role('owner','admin'),async(_,res)=>{
 const r=await pool.query("select id,network,public_key,status,created_at from treasury_accounts order by network");
 res.json(r.rows);
});

app.put('/api/admin/treasury',auth,role('owner','admin'),async(req,res)=>{
 const p=z.object({network:z.enum(['testnet','mainnet']),publicKey:z.string().regex(/^G[A-Z2-7]{55}$/),status:z.enum(['active','disabled']).default('active')}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid treasury Stellar address'});
 if(p.data.network!==STELLAR_NETWORK)return res.status(400).json({error:'Treasury network must match the active platform network',network:STELLAR_NETWORK});
 const r=await pool.query("insert into treasury_accounts(network,public_key,status) values($1,$2,$3) on conflict(network) do update set public_key=excluded.public_key,status=excluded.status returning id,network,public_key,status",[p.data.network,p.data.publicKey,p.data.status]);
 await audit(req,'treasury.updated',{network:p.data.network});
 res.json(r.rows[0]);
});

app.post('/api/admin/assets',auth,role('owner','admin'),async(req,res)=>{
 const p=z.object({
   symbol:z.string().min(1).max(12).regex(/^[A-Z0-9]+$/),
   issuer:z.string().regex(/^G[A-Z2-7]{55}$/).optional(),
   network:z.enum(['testnet','mainnet']),
   decimals:z.number().int().min(0).max(18).default(7),
   contractAddress:z.string().regex(/^C[A-Z2-7]{55}$/).optional(),
   status:z.enum(['active','disabled']).default('active')
 }).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid supported asset'});
 if(p.data.network!==STELLAR_NETWORK)return res.status(400).json({error:'Asset network must match the active platform network',network:STELLAR_NETWORK});
 const r=await pool.query(`insert into supported_assets(symbol,issuer,network,decimals,contract_address,status)
   values($1,$2,$3,$4,$5,$6)
   on conflict(symbol,network,issuer) do update set decimals=excluded.decimals,contract_address=excluded.contract_address,status=excluded.status
   returning id,symbol,issuer,network,decimals,contract_address,status`,
   [p.data.symbol,p.data.issuer||null,p.data.network,p.data.decimals,p.data.contractAddress||null,p.data.status]);
 await audit(req,'asset.updated',{symbol:p.data.symbol,network:p.data.network});
 res.status(201).json(r.rows[0]);
});

app.get('/api/admin/funding-orders',auth,role('owner','admin'),async(_,res)=>{
 const r=await pool.query(`select f.id,f.user_id,u.email,u.full_name,f.amount,f.stellar_hash,f.status,
   f.idempotency_key,f.metadata,f.created_at,f.confirmed_at,a.symbol,a.issuer,a.network,a.decimals,a.contract_address,
   t.public_key as treasury_public_key
   from funding_orders f
   join users u on u.id=f.user_id
   join supported_assets a on a.id=f.asset_id
   left join treasury_accounts t on t.id=f.treasury_account_id
   order by f.created_at desc limit 500`);
 res.json(r.rows);
});

app.post('/api/admin/funding-orders',auth,role('owner','admin'),async(req,res)=>{
 const p=z.object({
   userId:z.string().uuid(),
   assetId:z.string().uuid(),
   amount:z.number().finite().positive(),
   idempotencyKey:z.string().min(8).max(120)
 }).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid funding order'});
 const u=await pool.query("select id,kyc_status from users where id=$1",[p.data.userId]);
 if(!u.rowCount)return res.status(404).json({error:'Customer not found'});
 if(u.rows[0].kyc_status!=='verified')return res.status(409).json({error:'Customer must have verified KYC before funding'});
 const a=await pool.query("select * from supported_assets where id=$1 and status='active'",[p.data.assetId]);
 if(!a.rowCount)return res.status(404).json({error:'Supported asset not found or disabled'});
 if(a.rows[0].network!==STELLAR_NETWORK)return res.status(409).json({error:'Asset is not on the active network'});
 const w=await pool.query("select id,public_key from wallets where user_id=$1 and network=$2",[p.data.userId,STELLAR_NETWORK]);
 if(!w.rowCount)return res.status(409).json({error:'Customer must link a wallet before funding'});
 const t=await pool.query("select id,public_key from treasury_accounts where network=$1 and status='active'",[STELLAR_NETWORK]);
 if(!t.rowCount)return res.status(409).json({error:'Treasury account is not configured'});
 try{
   const r=await pool.query(`insert into funding_orders(user_id,treasury_account_id,asset_id,amount,status,idempotency_key,metadata)
     values($1,$2,$3,$4,'pending',$5,$6)
     returning id,user_id,asset_id,amount,status,idempotency_key,created_at`,
     [p.data.userId,t.rows[0].id,p.data.assetId,p.data.amount,p.data.idempotencyKey,JSON.stringify({recipient:w.rows[0].public_key,created_by:req.user.sub})]);
   await audit(req,'funding.order.created',{order_id:r.rows[0].id,user_id:p.data.userId,asset_id:p.data.assetId,amount:p.data.amount});
   res.status(201).json({...r.rows[0],recipient:w.rows[0].public_key,treasury_public_key:t.rows[0].public_key});
 }catch(e){
   if(e?.code==='23505')return res.status(409).json({error:'That idempotency key has already been used'});
   console.error(e);res.status(500).json({error:'Could not create funding order'});
 }
});

app.get('/api/admin/funding-orders/:id',auth,role('owner','admin'),async(req,res)=>{
 const r=await pool.query(`select f.id,f.user_id,f.amount,f.stellar_hash,f.status,f.idempotency_key,f.metadata,
   a.symbol,a.issuer,a.network,a.decimals,a.contract_address,t.public_key as treasury_public_key,
   w.public_key as recipient_public_key,u.email,u.full_name
   from funding_orders f join users u on u.id=f.user_id join supported_assets a on a.id=f.asset_id
   join treasury_accounts t on t.id=f.treasury_account_id
   join wallets w on w.user_id=f.user_id and w.network=a.network where f.id=$1`,[req.params.id]);
 if(!r.rowCount)return res.status(404).json({error:'Funding order not found'});
 res.json(r.rows[0]);
});

async function verifyClassicStellarPayment(hash,expected){
 const horizonUrl=expected.network==='mainnet'?'https://horizon.stellar.org':'https://horizon-testnet.stellar.org';
 const tr=await fetch(horizonUrl+'/transactions/'+encodeURIComponent(hash));
 if(tr.status===404) return {ok:false,error:'Transaction not found on Stellar'};
 if(!tr.ok) throw new Error('Horizon transaction lookup failed: HTTP '+tr.status);
 const tx=await tr.json();
 if(!tx.successful) return {ok:false,error:'Transaction is not successful'};
 const or=await fetch(horizonUrl+'/transactions/'+encodeURIComponent(hash)+'/operations?limit=200');
 if(!or.ok) throw new Error('Horizon operation lookup failed: HTTP '+or.status);
 const ops=(await or.json())._embedded?.records||[];
 const amount=Number(expected.amount);
 const match=ops.find(o=>{
   if(o.type!=='payment') return false;
   if(o.from!==expected.treasury_public_key || o.to!==expected.recipient_public_key) return false;
   const assetOk=o.asset_code===expected.symbol && o.asset_issuer===expected.issuer;
   return assetOk && Math.abs(Number(o.amount)-amount)<1e-7;
 });
 return match?{ok:true,operation:match,tx}:{ok:false,error:'No payment operation matched the treasury, customer wallet, asset and amount for this funding order'};
}

app.post('/api/admin/funding-orders/:id/confirm',auth,role('owner','admin'),async(req,res)=>{
 const p=z.object({stellarHash:z.string().regex(/^[a-f0-9]{64}$/i)}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Enter a valid Stellar transaction hash'});
 const o=await pool.query(`select f.*,a.symbol,a.issuer,a.network,a.decimals,a.contract_address,t.public_key as treasury_public_key,
   w.public_key as recipient_public_key
   from funding_orders f
   join supported_assets a on a.id=f.asset_id
   join treasury_accounts t on t.id=f.treasury_account_id
   join wallets w on w.user_id=f.user_id and w.network=f.metadata->>'network'
   where f.id=$1`,[req.params.id]);
 // Older orders may not have network in metadata; fall back to the active-network wallet.
 const order= o.rowCount ? o.rows[0] : (await pool.query(`select f.*,a.symbol,a.issuer,a.network,a.decimals,a.contract_address,t.public_key as treasury_public_key,
   w.public_key as recipient_public_key
   from funding_orders f join supported_assets a on a.id=f.asset_id join treasury_accounts t on t.id=f.treasury_account_id
   join wallets w on w.user_id=f.user_id and w.network=$2 where f.id=$1`,[req.params.id,STELLAR_NETWORK])).rows[0];
 if(!order)return res.status(404).json({error:'Funding order not found'});
 if(order.status==='confirmed')return res.status(409).json({error:'Funding order is already confirmed',stellar_hash:order.stellar_hash});
 let chain;
 try{chain=await stellarTransactionStatus(p.data.stellarHash)}
 catch(e){return res.status(503).json({error:'Could not verify the Stellar transaction yet. Try again shortly.'})}
 if(chain.status!=='SUCCESS')return res.status(409).json({error:'Stellar transaction is not successful yet',chain_status:chain.status});
 let payment;
 try{payment=await verifyClassicStellarPayment(p.data.stellarHash,order)}
 catch(e){return res.status(503).json({error:'Could not verify the Stellar payment operation yet. Try again shortly.'})}
 if(!payment.ok)return res.status(409).json({error:payment.error});
 const r=await pool.query(`update funding_orders set stellar_hash=$2,status='confirmed',confirmed_at=now(),
   metadata=metadata||$3::jsonb where id=$1 returning id,user_id,amount,stellar_hash,status,created_at,confirmed_at`,
   [order.id,p.data.stellarHash,JSON.stringify({verified_rpc_status:chain.status,recipient:order.recipient_public_key,asset:order.symbol,issuer:order.issuer,verification_level:'operation-verified',operation_id:payment.operation.id})]);
 await audit(req,'funding.order.confirmed',{order_id:order.id,stellar_hash:p.data.stellarHash,verification_level:'transaction-success-only'});
 res.json({...r.rows[0],verification_level:'operation-verified'});
});

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
 const p=z.object({status:z.enum(['pending','verified','rejected']),note:z.string().max(1000).optional()}).safeParse(req.body);
 if(!p.success)return res.status(400).json({error:'Invalid KYC status'});
 const u=await pool.query('select id,kyc_status from users where id=$1',[req.params.id]);
 if(!u.rowCount)return res.status(404).json({error:'User not found'});
 if(p.data.status!=='pending' && !p.data.note?.trim())return res.status(400).json({error:'A review note is required when changing KYC status'});
 const session=await pool.query("select id from kyc_sessions where user_id=$1 order by created_at desc limit 1",[req.params.id]);
 if(!session.rowCount && p.data.status==='verified')return res.status(409).json({error:'Cannot verify KYC without a KYC session'});
 await pool.query('update users set kyc_status=$1,updated_at=now() where id=$2',[p.data.status,req.params.id]);
 await pool.query("update kyc_sessions set status=$1,updated_at=now(),result=result||$2::jsonb where id=(select id from kyc_sessions where user_id=$3 order by created_at desc limit 1)",[p.data.status,JSON.stringify({manual_review:true,note:p.data.note||null,reviewed_at:new Date().toISOString(),reviewed_by:req.user.sub}),req.params.id]);
 await audit(req,'kyc.status.changed',{target:req.params.id,status:p.data.status,note:p.data.note||null});
 res.json({ok:true,status:p.data.status});
});
app.use((err,_,res,next)=>{console.error(err);if(res.headersSent)return next(err);res.status(500).json({error:'Internal server error'})});
const runScheduledReconciliation=async()=>{try{const {stdout,stderr}=await execFileAsync(process.execPath,['src/reconcile.js'],{cwd:new URL('..',import.meta.url).pathname,timeout:120000,maxBuffer:1024*1024});console.log('Scheduled reconciliation completed',stdout.trim(),stderr.trim()||'')}catch(e){console.error('Scheduled reconciliation failed',e?.message||e)}};
const reconciliationEnabled=process.env.ENABLE_INTERNAL_RECONCILIATION!=='false';
app.listen(Number(process.env.PORT||8080),()=>{console.log('Liholiswano API started');if(reconciliationEnabled){setTimeout(runScheduledReconciliation,60000);setInterval(runScheduledReconciliation,15*60*1000);console.log('Internal reconciliation scheduler enabled (15-minute interval)')}});
