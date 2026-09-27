import 'dotenv/config';
import fs from 'fs';
import {Pool} from 'pg';
import bcrypt from 'bcryptjs';

if (!process.env.DATABASE_URL) throw new Error('DATABASE_URL is required');

const pool=new Pool({
  connectionString:process.env.DATABASE_URL,
  ssl:process.env.NODE_ENV==='production'?{rejectUnauthorized:false}:false
});

try {
  const sql=fs.readFileSync(new URL('./schema.sql', import.meta.url),'utf8');
  await pool.query(sql);
  if(process.env.BOOTSTRAP_OWNER_EMAIL && process.env.BOOTSTRAP_OWNER_PASSWORD){
    const email=process.env.BOOTSTRAP_OWNER_EMAIL.toLowerCase().trim();
    const passwordHash=await bcrypt.hash(process.env.BOOTSTRAP_OWNER_PASSWORD,12);
    await pool.query(`
      insert into users(email,password_hash,full_name,country,role,kyc_status,email_verified)
      values($1,$2,'Liholiswano Platform Owner','BW','owner','verified',true)
      on conflict(email) do update set role='owner',kyc_status='verified',email_verified=true,updated_at=now()
    `,[email,passwordHash]);
    console.log('Bootstrap owner account is ready.');
  }
  console.log('Database schema is ready.');
} finally {
  await pool.end();
}
