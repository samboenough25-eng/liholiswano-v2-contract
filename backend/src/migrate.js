import 'dotenv/config';
import fs from 'fs';
import {Pool} from 'pg';

if (!process.env.DATABASE_URL) throw new Error('DATABASE_URL is required');

const pool=new Pool({
  connectionString:process.env.DATABASE_URL,
  ssl:process.env.NODE_ENV==='production'?{rejectUnauthorized:false}:false
});

try {
  const sql=fs.readFileSync(new URL('./schema.sql', import.meta.url),'utf8');
  await pool.query(sql);
  console.log('Database schema is ready.');
} finally {
  await pool.end();
}
