import 'dotenv/config';
import {Pool} from 'pg';

const pool=new Pool({
  connectionString:process.env.DATABASE_URL,
  ssl:process.env.NODE_ENV==='production'?{rejectUnauthorized:false}:false
});
const rpcUrl=process.env.STELLAR_RPC_URL||'https://soroban-testnet.stellar.org';

async function rpc(method,params){
  const r=await fetch(rpcUrl,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:Date.now(),method,params})});
  if(!r.ok)throw new Error('Stellar RPC HTTP '+r.status);
  const j=await r.json();
  if(j.error)throw new Error(j.error.message||'Stellar RPC error');
  return j.result;
}
try{
  const q=await pool.query("select id,stellar_hash from transactions where stellar_hash is not null and status in ('pending','confirmed') order by created_at desc limit 250");
  let checked=0,confirmed=0,failed=0,pending=0;
  for(const row of q.rows){
    try{
      const result=await rpc('getTransaction',{hash:row.stellar_hash});
      checked++;
      if(result.status==='SUCCESS'){
        await pool.query("update transactions set status='confirmed',metadata=metadata||$2::jsonb where id=$1",[row.id,JSON.stringify({reconciled_at:new Date().toISOString(),rpc_status:result.status,ledger:result.ledger})]);
        confirmed++;
      }else if(result.status==='FAILED'){
        await pool.query("update transactions set status='failed',metadata=metadata||$2::jsonb where id=$1",[row.id,JSON.stringify({reconciled_at:new Date().toISOString(),rpc_status:result.status})]);
        failed++;
      }else{
        await pool.query("update transactions set status='pending',metadata=metadata||$2::jsonb where id=$1",[row.id,JSON.stringify({reconciled_at:new Date().toISOString(),rpc_status:result.status})]);
        pending++;
      }
    }catch(e){
      console.error('reconcile',row.stellar_hash,e.message);
    }
  }
  console.log(JSON.stringify({checked,confirmed,failed,pending}));
}finally{
  await pool.end();
}
