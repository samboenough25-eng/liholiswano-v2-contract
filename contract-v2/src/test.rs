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
    assert_eq!(state.reserve, 500); // m3's seized collateral
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
