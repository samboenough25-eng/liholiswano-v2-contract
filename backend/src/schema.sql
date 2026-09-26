create extension if not exists pgcrypto;
create table if not exists users(id uuid primary key default gen_random_uuid(),email text unique not null,password_hash text not null,full_name text not null,country text not null check(country in ('BW','SZ')),role text not null default 'customer',kyc_status text not null default 'pending' check(kyc_status in ('pending','verified','rejected')),kyc_reference text,created_at timestamptz not null default now(),updated_at timestamptz not null default now());
alter table users drop constraint if exists users_role_check;
alter table users add constraint users_role_check check(role in ('customer','owner','admin','compliance'));
create table if not exists wallets(id uuid primary key default gen_random_uuid(),user_id uuid not null references users(id) on delete cascade,network text not null,public_key text not null,created_at timestamptz not null default now(),unique(user_id,network),unique(public_key,network));
create table if not exists groups(id text primary key,contract_id text not null,admin_user_id uuid references users(id),status text not null default 'open',created_at timestamptz not null default now());
create table if not exists memberships(group_id text references groups(id) on delete cascade,user_id uuid references users(id) on delete cascade,status text not null default 'active',joined_at timestamptz not null default now(),primary key(group_id,user_id));
create table if not exists transactions(id uuid primary key default gen_random_uuid(),user_id uuid references users(id),group_id text references groups(id),type text not null,asset text,amount numeric(30,7),stellar_hash text,status text not null default 'pending',metadata jsonb not null default '{}'::jsonb,created_at timestamptz not null default now());
create table if not exists notifications(id uuid primary key default gen_random_uuid(),user_id uuid references users(id) on delete cascade,channel text not null,title text not null,body text not null,read_at timestamptz,created_at timestamptz not null default now());
create table if not exists audit_log(id bigserial primary key,user_id uuid references users(id),action text not null,ip inet,metadata jsonb not null default '{}'::jsonb,created_at timestamptz not null default now());
create index if not exists tx_user_created on transactions(user_id,created_at desc);
create index if not exists audit_created on audit_log(created_at desc);

alter table users add column if not exists email_verified boolean not null default false;
alter table users add column if not exists phone text;
alter table users add column if not exists phone_verified boolean not null default false;
alter table users drop constraint if exists users_kyc_status_check;
alter table users add constraint users_kyc_status_check check(kyc_status in ('pending','in_progress','verified','rejected','review'));

create table if not exists kyc_sessions(
 id uuid primary key default gen_random_uuid(),
 user_id uuid not null references users(id) on delete cascade,
 provider text not null,
 provider_session_id text,
 reference text unique not null,
 status text not null default 'pending',
 document_type text,
 document_verified boolean not null default false,
 face_verified boolean not null default false,
 liveness_verified boolean not null default false,
 aml_screened boolean not null default false,
 pep_screened boolean not null default false,
 duplicate_face_checked boolean not null default false,
 result jsonb not null default '{}'::jsonb,
 created_at timestamptz not null default now(),
 updated_at timestamptz not null default now()
);
create table if not exists kyc_events(
 id bigserial primary key,
 session_id uuid references kyc_sessions(id) on delete cascade,
 user_id uuid references users(id) on delete cascade,
 event_type text not null,
 provider_event_id text unique,
 payload jsonb not null default '{}'::jsonb,
 created_at timestamptz not null default now()
);
create index if not exists kyc_sessions_user on kyc_sessions(user_id,created_at desc);
create index if not exists kyc_events_session on kyc_events(session_id,created_at desc);

create table if not exists verification_tokens(
 id uuid primary key default gen_random_uuid(),
 user_id uuid not null references users(id) on delete cascade,
 channel text not null check(channel in ('email','phone')),
 token_hash text not null,
 expires_at timestamptz not null,
 used_at timestamptz,
 created_at timestamptz not null default now()
);
create index if not exists verification_tokens_user on verification_tokens(user_id,channel,created_at desc);
