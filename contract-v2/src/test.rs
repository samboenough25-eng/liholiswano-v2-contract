#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{token, Env};

const DAY: u64 = 24 * 60 * 60;

fn setup<'a>(env: &Env) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    let token_client = token::Client::new(env, &sac);
    let asset_client = token::StellarAssetClient::new(env, &sac);
    (admin, token_client, asset_client)
}

fn deploy(env: &Env) -> Address {
    env.register_contract(None, LiholiswanoContractV2)
}

/// Initializes the contract with a fresh protocol admin and approves
/// `token` on the stablecoin allowlist — the setup every test needs before
/// `create_group` will accept that token.
fn init_and_approve(env: &Env, contract_id: &Address, token: &Address) {
    let client = LiholiswanoContractV2Client::new(env, contract_id);
    let protocol_admin = Address::generate(env);
    client.initialize(&protocol_admin);
    client.add_approved_token(&protocol_admin, token);
}

fn advance_time(env: &Env, by_secs: u64) {
    let now = env.ledger().timestamp();
    env.ledger().set(LedgerInfo {
        timestamp: now + by_secs,
        protocol_version: 20,
        sequence_number: env.ledger().sequence(),
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16 * 4096,
        min_persistent_entry_ttl: 4096,
        max_entry_ttl: 6312000,
    });
}

#[test]
fn create_join_lock_happy_path_with_multi_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (token_admin, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    for m in [&m1, &m2, &m3] {
        asset_client.mint(m, &1_000);
    }
    let _ = token_admin;

    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone(), admin3.clone()]);
    client.create_group(
        &id,
        &admins,
        &2, // 2-of-3 threshold
        &token_client.address,
        &100,
        &500,
        &2000,
        &10,
        &(28 * DAY),
    );

    client.join_group(&id, &m1);
    client.join_group(&id, &m2);
    client.join_group(&id, &m3);

    // Lock with only 2 of the 3 admins co-signing — should succeed at threshold 2.
    let signing_admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
    client.lock_group(&id, &signing_admins);

    let state = client.get_group_state(&id);
    assert!(state.locked);
    assert_eq!(state.round, 1);
    assert_eq!(state.round_deadline, 28 * DAY);
}

#[test]
fn lock_fails_with_too_few_admin_signatures() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    for m in [&m1, &m2, &m3] {
        asset_client.mint(m, &1_000);
    }

    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
    client.create_group(
        &id, &admins, &2, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    client.join_group(&id, &m1);
    client.join_group(&id, &m2);
    client.join_group(&id, &m3);

    let only_one = Vec::from_array(&env, [admin1.clone()]);
    let result = client.try_lock_group(&id, &only_one);
    assert_eq!(result, Err(Ok(Error::NotEnoughAdminSignatures)));
}

#[test]
fn auto_default_after_deadline_and_waitlist_promotion_fills_the_slot() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    let waiter = Address::generate(&env);
    for m in [&m1, &m2, &m3, &waiter] {
        asset_client.mint(m, &1_000);
    }

    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin.clone()]);
    client.create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    client.join_group(&id, &m1);
    client.join_group(&id, &m2);
    client.join_group(&id, &m3);

    // Someone queues up before the group is even locked.
    client.join_waitlist(&id, &waiter);

    client.lock_group(&id, &admins);

    // m1 and m2 do their part; m3 goes silent.
    client.contribute(&id, &m1);
    client.contribute(&id, &m2);
    client.submit_bid(&id, &m1, &0);
    client.submit_bid(&id, &m2, &0);

    // Before the deadline, settling should still fail — m3 hasn't acted.
    let too_early = client.try_settle_round(&id);
    assert_eq!(too_early, Err(Ok(Error::NotAllContributed)));

    // Fast-forward past the 28-day deadline.
    advance_time(&env, 28 * DAY + 1);

    // Now anyone can settle; m3 is auto-defaulted instead of blocking it.
    client.settle_round(&id);

    let state = client.get_group_state(&id);
    let m3_state = client.get_member(&id, &m3);
    assert!(m3_state.defaulted);
    assert!(!m3_state.active);
    // m3's 500 collateral was seized into the reserve, and settling this
    // round immediately topped the pot up by the 100 m3 failed to pay.
    assert_eq!(state.reserve, 400);
    assert_eq!(state.total_uncovered_shortfall, 0);
    assert_eq!(state.open_slots, 1);
    assert_eq!(active_count(&state.members), 2);

    // The waitlisted member takes the freed slot.
    let balance_before_promotion = token_client.balance(&contract_id);
    client.promote_from_waitlist(&id, &waiter);
    let state = client.get_group_state(&id);
    assert_eq!(state.open_slots, 0);
    assert_eq!(active_count(&state.members), 3);
    // Promotion adds exactly one fresh collateral transfer into the contract.
    assert_eq!(token_client.balance(&contract_id), balance_before_promotion + 500);
}

#[test]
fn promote_out_of_turn_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    let first_in_line = Address::generate(&env);
    let second_in_line = Address::generate(&env);
    for m in [&m1, &m2, &m3, &first_in_line, &second_in_line] {
        asset_client.mint(m, &1_000);
    }

    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin.clone()]);
    client.create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    client.join_group(&id, &m1);
    client.join_group(&id, &m2);
    client.join_group(&id, &m3);
    client.join_waitlist(&id, &first_in_line);
    client.join_waitlist(&id, &second_in_line);
    client.lock_group(&id, &admins);

    client.mark_default(&id, &admins, &m3);

    let result = client.try_promote_from_waitlist(&id, &second_in_line);
    assert_eq!(result, Err(Ok(Error::NotNextInWaitlist)));

    // The rightful next-in-line succeeds.
    client.promote_from_waitlist(&id, &first_in_line);
    let state = client.get_group_state(&id);
    assert_eq!(state.waitlist.len(), 1);
    assert_eq!(state.waitlist.get(0).unwrap(), second_in_line);
}

#[test]
fn settle_round_conserves_value_with_uneven_split() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    for m in [&m1, &m2, &m3] {
        asset_client.mint(m, &1_000);
    }

    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin.clone()]);
    client.create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    client.join_group(&id, &m1);
    client.join_group(&id, &m2);
    client.join_group(&id, &m3);
    client.lock_group(&id, &admins);

    for m in [&m1, &m2, &m3] {
        client.contribute(&id, m);
    }
    client.submit_bid(&id, &m1, &1000); // 10%
    client.submit_bid(&id, &m2, &0);
    client.submit_bid(&id, &m3, &0);

    let contract_balance_before = token_client.balance(&contract_id);
    client.settle_round(&id);
    let contract_balance_after = token_client.balance(&contract_id);

    // pot = 300, bid = 30 (10%), net_to_winner = 270, split 30 between 2
    // others = 15 each, remainder 0 to reserve. The 300 pot was already
    // inside the contract (from the three contribute() calls) before
    // settlement — settling only pays 270 + 15 + 15 = 300 of it back out.
    let state = client.get_group_state(&id);
    assert_eq!(state.reserve, 0);
    assert_eq!(contract_balance_before - contract_balance_after, 270 + 15 + 15);
}

#[test]
fn cannot_join_after_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    let latecomer = Address::generate(&env);
    for m in [&m1, &m2, &m3, &latecomer] {
        asset_client.mint(m, &1_000);
    }

    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin.clone()]);
    client.create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    client.join_group(&id, &m1);
    client.join_group(&id, &m2);
    client.join_group(&id, &m3);
    client.lock_group(&id, &admins);

    let result = client.try_join_group(&id, &latecomer);
    assert_eq!(result, Err(Ok(Error::AlreadyLocked)));
}

#[test]
fn non_admin_cannot_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let impostor = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    for m in [&m1, &m2, &m3] {
        asset_client.mint(m, &1_000);
    }

    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin.clone()]);
    client.create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    client.join_group(&id, &m1);
    client.join_group(&id, &m2);
    client.join_group(&id, &m3);

    let bogus = Vec::from_array(&env, [impostor.clone()]);
    let result = client.try_lock_group(&id, &bogus);
    assert_eq!(result, Err(Ok(Error::NotAdmin)));
}

#[test]
fn reading_unknown_group_fails() {
    let env = Env::default();
    let contract_id = deploy(&env);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);
    let id = Symbol::new(&env, "nope");
    let result = client.try_get_group_state(&id);
    assert_eq!(result, Err(Ok(Error::GroupNotFound)));
}

#[test]
fn create_group_rejects_unapproved_token() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    // Initialize, but deliberately do NOT approve token_client's address —
    // create_group must refuse it.
    let protocol_admin = Address::generate(&env);
    client.initialize(&protocol_admin);

    let admin = Address::generate(&env);
    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin.clone()]);
    let result = client.try_create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    assert_eq!(result, Err(Ok(Error::TokenNotApproved)));

    // Sanity: once approved, the identical call succeeds.
    client.add_approved_token(&protocol_admin, &token_client.address);
    let m1 = Address::generate(&env);
    asset_client.mint(&m1, &10_000);
    let ok = client.try_create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    assert!(ok.is_ok());
}

#[test]
fn create_group_before_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, _) = setup(&env);
    let contract_id = deploy(&env);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin.clone()]);
    let result = client.try_create_group(
        &id, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY),
    );
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

#[test]
fn non_protocol_admin_cannot_approve_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, _) = setup(&env);
    let contract_id = deploy(&env);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let protocol_admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.initialize(&protocol_admin);

    let result = client.try_add_approved_token(&stranger, &token_client.address);
    assert_eq!(result, Err(Ok(Error::NotProtocolAdmin)));
}

#[test]
fn cannot_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = deploy(&env);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    client.initialize(&admin1);
    let result = client.try_initialize(&admin2);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn list_groups_enumerates_every_created_group_for_automation() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, token_client, asset_client) = setup(&env);
    let contract_id = deploy(&env);
    init_and_approve(&env, &contract_id, &token_client.address);
    let client = LiholiswanoContractV2Client::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);

    let id1 = Symbol::new(&env, "grp1");
    let id2 = Symbol::new(&env, "grp2");
    client.create_group(&id1, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY));
    client.create_group(&id2, &admins, &1, &token_client.address, &100, &500, &2000, &10, &(28 * DAY));

    let groups = client.list_groups();
    assert_eq!(groups.len(), 2);
    assert!(groups.contains(&id1));
    assert!(groups.contains(&id2));
    let _ = asset_client; // unused in this test beyond setup's return shape
}


// ───────────────────── V2.2 tests: money exits, reserve, limits ─────────────────────

struct Fixture<'a> {
    env: &'a Env,
    cid: Address,
    tok: token::Client<'a>,
    id: Symbol,
    admins: Vec<Address>,
    ms: [Address; 3],
}

fn fixture<'a>(env: &'a Env) -> Fixture<'a> {
    env.mock_all_auths();
    let (_, tok, asset) = setup(env);
    let cid = deploy(env);
    init_and_approve(env, &cid, &tok.address);
    let client = LiholiswanoContractV2Client::new(env, &cid);
    let admin = Address::generate(env);
    let ms = [Address::generate(env), Address::generate(env), Address::generate(env)];
    for m in ms.iter() {
        asset.mint(m, &10_000);
    }
    let id = Symbol::new(env, "grp1");
    let admins = Vec::from_array(env, [admin]);
    client.create_group(&id, &admins, &1, &tok.address, &100, &500, &2000, &10, &(28 * DAY));
    for m in ms.iter() {
        client.join_group(&id, m);
    }
    client.lock_group(&id, &admins);
    Fixture { env, cid, tok, id, admins, ms }
}

fn play_round(f: &Fixture, bids: [u32; 3]) {
    let client = LiholiswanoContractV2Client::new(f.env, &f.cid);
    for m in f.ms.iter() {
        client.contribute(&f.id, m);
    }
    for (m, b) in f.ms.iter().zip(bids.iter()) {
        // a member who already won this rotation can't bid; that is fine
        let _ = client.try_submit_bid(&f.id, m, b);
    }
    client.settle_round(&f.id);
}

#[test]
fn full_cycle_completes_and_every_unit_leaves_the_contract() {
    let env = Env::default();
    let f = fixture(&env);
    let client = LiholiswanoContractV2Client::new(&env, &f.cid);

    play_round(&f, [0, 0, 0]);
    play_round(&f, [0, 0, 0]);
    assert!(!client.get_group_state(&f.id).completed);
    play_round(&f, [0, 0, 0]);

    let st = client.get_group_state(&f.id);
    assert!(st.completed, "cycle must complete once all 3 have won");
    assert_eq!(st.refund_share, 0);

    // Contributions can no longer be made, and settle is closed.
    assert_eq!(client.try_contribute(&f.id, &f.ms[0]), Err(Ok(Error::AlreadyCompleted)));
    assert_eq!(client.try_settle_round(&f.id), Err(Ok(Error::AlreadyCompleted)));

    for m in f.ms.iter() {
        client.claim_refund(&f.id, m);
    }
    // Everyone is exactly whole again: paid 300 in, won 300, got 500 back.
    for m in f.ms.iter() {
        assert_eq!(f.tok.balance(m), 10_000);
    }
    assert_eq!(f.tok.balance(&f.cid), 0);
}

#[test]
fn refund_cannot_be_claimed_twice_or_early() {
    let env = Env::default();
    let f = fixture(&env);
    let client = LiholiswanoContractV2Client::new(&env, &f.cid);
    assert_eq!(client.try_claim_refund(&f.id, &f.ms[0]), Err(Ok(Error::NotCompleted)));
    play_round(&f, [0, 0, 0]);
    play_round(&f, [0, 0, 0]);
    play_round(&f, [0, 0, 0]);
    client.claim_refund(&f.id, &f.ms[0]);
    assert_eq!(client.try_claim_refund(&f.id, &f.ms[0]), Err(Ok(Error::AlreadyClaimed)));
}

#[test]
fn reserve_tops_up_pot_when_a_winner_defaults_and_leftover_is_refunded() {
    let env = Env::default();
    let f = fixture(&env);
    let client = LiholiswanoContractV2Client::new(&env, &f.cid);

    // Round 1: ms[0] wins (bids 5% -> pays 15 of the 300 pot, split 7/7, 1 dust to reserve).
    for m in f.ms.iter() {
        client.contribute(&f.id, m);
    }
    client.submit_bid(&f.id, &f.ms[0], &500);
    client.submit_bid(&f.id, &f.ms[1], &0);
    client.submit_bid(&f.id, &f.ms[2], &0);
    client.settle_round(&f.id);
    assert_eq!(client.get_group_state(&f.id).reserve, 1);

    // Round 2: ms[0] (already paid out) stops paying.
    client.contribute(&f.id, &f.ms[1]);
    client.contribute(&f.id, &f.ms[2]);
    client.submit_bid(&f.id, &f.ms[1], &0);
    client.submit_bid(&f.id, &f.ms[2], &0);
    advance_time(&env, 29 * DAY);
    let before = f.tok.balance(&f.cid);
    client.settle_round(&f.id);
    // Winner still receives a FULL 300 pot: 200 collected + 100 from the reserve.
    assert_eq!(before - f.tok.balance(&f.cid), 300);
    let st = client.get_group_state(&f.id);
    assert_eq!(st.total_uncovered_shortfall, 0);
    assert_eq!(st.reserve, 1 + 500 - 100);

    // Round 3: last un-won member wins, again a full pot, then the cycle closes.
    client.contribute(&f.id, &f.ms[1]);
    client.contribute(&f.id, &f.ms[2]);
    let _ = client.try_submit_bid(&f.id, &f.ms[1], &0);
    let _ = client.try_submit_bid(&f.id, &f.ms[2], &0);
    client.settle_round(&f.id);
    let st = client.get_group_state(&f.id);
    assert!(st.completed);
    assert_eq!(st.total_uncovered_shortfall, 0);

    // Survivors (ms[1], ms[2]) split what's left; ms[0] forfeited everything.
    assert_eq!(client.try_claim_refund(&f.id, &f.ms[0]), Err(Ok(Error::MemberNotActive)));
    client.claim_refund(&f.id, &f.ms[1]);
    client.claim_refund(&f.id, &f.ms[2]);
    // Whatever remains in the contract is exactly the reserve dust — nothing is stranded.
    let st = client.get_group_state(&f.id);
    assert_eq!(f.tok.balance(&f.cid), st.reserve);
    assert!(st.reserve < 2, "at most sub-member dust may remain");
}

#[test]
fn two_defaults_still_pay_full_pots_and_conserve_value_when_collateral_rule_holds() {
    // 4 members, contribution 100, collateral 300 (= the lock-time minimum, 3*100).
    // Because collateral >= (n-1)*contribution, a defaulter's own collateral
    // covers every contribution their seat misses, so no bad debt can arise.
    let env = Env::default();
    env.mock_all_auths();
    let (_, tok, asset) = setup(&env);
    let cid = deploy(&env);
    init_and_approve(&env, &cid, &tok.address);
    let client = LiholiswanoContractV2Client::new(&env, &cid);
    let admin = Address::generate(&env);
    let ms = [Address::generate(&env), Address::generate(&env), Address::generate(&env), Address::generate(&env)];
    for m in ms.iter() { asset.mint(m, &10_000); }
    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin]);
    client.create_group(&id, &admins, &1, &tok.address, &100, &300, &2000, &10, &(28 * DAY));
    for m in ms.iter() { client.join_group(&id, m); }
    client.lock_group(&id, &admins);

    // Round 1: all pay, ms[0] wins (tie on 0% -> lowest index).
    for m in ms.iter() { client.contribute(&id, m); }
    for m in ms.iter() { client.submit_bid(&id, m, &0); }
    client.settle_round(&id);

    // Round 2: ms[0] and ms[1] go silent, ms[2] and ms[3] pay.
    client.contribute(&id, &ms[2]);
    client.contribute(&id, &ms[3]);
    client.submit_bid(&id, &ms[2], &0);
    client.submit_bid(&id, &ms[3], &0);
    advance_time(&env, 29 * DAY);
    let before = tok.balance(&cid);
    client.settle_round(&id);
    assert_eq!(before - tok.balance(&cid), 400, "winner still gets a full 400 pot");
    assert_eq!(client.get_group_state(&id).reserve, 600 - 200);

    // Round 3: the last un-won survivor (ms[3]) wins another full pot; cycle closes.
    client.contribute(&id, &ms[2]);
    client.contribute(&id, &ms[3]);
    let _ = client.try_submit_bid(&id, &ms[2], &0);
    client.submit_bid(&id, &ms[3], &0);
    client.settle_round(&id);
    let st = client.get_group_state(&id);
    assert!(st.completed);
    assert_eq!(st.total_uncovered_shortfall, 0);
    assert_eq!(st.refund_share, 100); // (600 - 200 - 200) / 2 survivors

    client.claim_refund(&id, &ms[2]);
    client.claim_refund(&id, &ms[3]);
    assert_eq!(client.try_claim_refund(&id, &ms[1]), Err(Ok(Error::MemberNotActive)));
    assert_eq!(tok.balance(&cid), client.get_group_state(&id).reserve);
    assert_eq!(client.get_group_state(&id).reserve, 0);
}

#[test]
fn lock_rejects_collateral_that_cannot_cover_a_first_slot_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, tok, asset) = setup(&env);
    let cid = deploy(&env);
    init_and_approve(&env, &cid, &tok.address);
    let client = LiholiswanoContractV2Client::new(&env, &cid);
    let admin = Address::generate(&env);
    let ms = [Address::generate(&env), Address::generate(&env), Address::generate(&env)];
    for m in ms.iter() { asset.mint(m, &10_000); }
    let id = Symbol::new(&env, "grp1");
    let admins = Vec::from_array(&env, [admin]);
    // 3 members * contribution 100 -> a first winner could still owe 200; collateral 150 is too low.
    client.create_group(&id, &admins, &1, &tok.address, &100, &150, &2000, &10, &(28 * DAY));
    for m in ms.iter() { client.join_group(&id, m); }
    assert_eq!(client.try_lock_group(&id, &admins), Err(Ok(Error::CollateralTooLow)));
}

#[test]
fn max_members_is_capped() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, tok, _) = setup(&env);
    let cid = deploy(&env);
    init_and_approve(&env, &cid, &tok.address);
    let client = LiholiswanoContractV2Client::new(&env, &cid);
    let admins = Vec::from_array(&env, [Address::generate(&env)]);
    let id = Symbol::new(&env, "big");
    let r = client.try_create_group(&id, &admins, &1, &tok.address, &100, &50_000, &2000, &(MAX_MEMBERS + 1), &(28 * DAY));
    assert_eq!(r, Err(Ok(Error::InvalidConfig)));
    client.create_group(&id, &admins, &1, &tok.address, &100, &50_000, &2000, &MAX_MEMBERS, &(28 * DAY));
}

#[test]
fn manual_default_of_a_contributed_member_forfeits_instead_of_stranding() {
    let env = Env::default();
    let f = fixture(&env);
    let client = LiholiswanoContractV2Client::new(&env, &f.cid);
    client.contribute(&f.id, &f.ms[0]);
    let before_reserve = client.get_group_state(&f.id).reserve;
    client.mark_default(&f.id, &f.admins, &f.ms[0]);
    let st = client.get_group_state(&f.id);
    // 500 collateral + the 100 already contributed this round.
    assert_eq!(st.reserve - before_reserve, 600);
    assert_eq!(st.open_slots, 1);
}
