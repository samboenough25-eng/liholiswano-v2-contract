import smileIdentityCore from 'smile-identity-core';

const {WebApi, Signature}=smileIdentityCore;

export function kycConfigured(){
  return Boolean(process.env.SMILE_PARTNER_ID && process.env.SMILE_API_KEY);
}

function requireConfig(){
  if(!kycConfigured()) throw new Error('Smile ID is not configured');
}

function webApi(){
  requireConfig();
  const partnerId=String(process.env.SMILE_PARTNER_ID);
  const callback=process.env.KYC_CALLBACK_URL;
  if(!callback) throw new Error('KYC_CALLBACK_URL is required');
  const apiKey=process.env.SMILE_API_KEY;
  const server=process.env.SMILE_SERVER_URL || 'https://api.smileidentity.com';
  return new WebApi(partnerId,callback,apiKey,server);
}

export async function createHostedKycSession({userId,jobId,product='enhanced_kyc'}){
  const connection=webApi();
  return connection.get_web_token({
    user_id:userId,
    job_id:jobId,
    product,
    callback_url:process.env.KYC_CALLBACK_URL,
  });
}

export function confirmCallbackSignature(timestamp,signature){
  requireConfig();
  if(!timestamp || !signature) return false;
  const connection=new Signature(String(process.env.SMILE_PARTNER_ID),process.env.SMILE_API_KEY);
  return connection.confirm_signature(timestamp,signature);
}

export function normalizeKycDecision(payload){
  const results=Array.isArray(payload?.Actions)?payload.Actions:[];
  const first=results[0]||payload||{};
  const code=String(first.ResultCode ?? payload?.ResultCode ?? '');
  const final=String(first.IsFinalResult ?? payload?.IsFinalResult ?? '').toLowerCase()==='true';
  const text=String(first.ResultText ?? payload?.ResultText ?? payload?.message ?? '');
  const successCodes=new Set(['0810','0840']);
  const decision=successCodes.has(code)?'verified':(
    /pass|clear|approved|success/i.test(text) ? 'verified' :
    /fail|reject|block|denied/i.test(text) ? 'rejected' :
    final ? 'pending' : 'pending'
  );
  return {
    status:decision,
    resultCode:code||null,
    resultText:text||null,
    final,
    smileJobId:first.SmileJobID ?? payload?.SmileJobID ?? null,
    raw:payload,
  };
}
