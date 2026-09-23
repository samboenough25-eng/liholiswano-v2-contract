//! Liholiswano Protocol V2.1 — Soroban contract, expanded pilot scope.
//!
//! Builds on the pilot contract by adding the four features the first
//! pilot deliberately deferred (see PILOT_SCOPE.md):
//!   1. An automatic round deadline (configurable; 28 days is the default
//!      a group would normally choose), enforced on-chain.
//!   2. Automatic default detection: once the deadline passes, anyone can
//!      call `settle_round` and stragglers are defaulted automatically —
//!      no admin has to notice and act.
//!   3. A waitlist: addresses can queue to join a locked/full group, and
//!      are promoted (in order) into any slot a default frees up.
//!   4. Multi-admin control: group admin actions require a configurable
//!      N-of-M threshold of admin signatures in the same transaction,
//!      instead of trusting a single key.
//!
//! Money-safety rules carried over unchanged from the validated simulation
//! engine and the first pilot contract:
//! - Bid sacrifice splits use integer division (floor); any remainder goes
//!   to the group's reserve, never invented, never dropped.
//! - A defaulting member's seized collateral covers what they still owe;
//!   any amount beyond that is logged as an explicit uncovered shortfall
//!   (bad debt), never fabricated from the reserve or from other members.

#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env, Symbol, Vec,
};

// ── Errors ──────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    GroupAlreadyExists = 1,
    GroupNotFound = 2,
    NotAdmin = 3,
    AlreadyLocked = 4,
    NotLocked = 5,
    GroupFull = 6,
    AlreadyMember = 7,
    NotAMember = 8,
    TooFewMembersToLock = 9,
    BidTooHigh = 10,
    AlreadyBidThisRound = 11,
    AlreadyContributedThisRound = 12,
    NotAllContributed = 13,
    NotAllBid = 14,
    MemberNotActive = 15,
    MemberAlreadyDefaulted = 16,
    NothingToSettle = 17,
    InvalidConfig = 18,
    NotEligibleThisRotation = 19,
    NotEnoughAdminSignatures = 20,
    DuplicateAdmin = 21,
    AlreadyOnWaitlist = 22,
    NotNextInWaitlist = 23,
    NoOpenSlots = 24,
    NotInitialized = 25,
    AlreadyInitialized = 26,
    TokenNotApproved = 27,
    NotProtocolAdmin = 28,
}

// ── Storage types ───────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GroupConfig {
    pub admins: Vec<Address>,      // the fixed set of addresses eligible to act as admin
    pub admin_threshold: u32,      // how many of `admins` must co-sign an admin action
    pub token: Address,            // the asset this group contributes/pays out in
    pub contribution: i128,        // per member, per round, in the token's base units
    pub collateral: i128,          // flat collateral required to join
    pub max_bid_bps: u32,          // max bid, in basis points of the pot (2000 = 20%)
    pub max_members: u32,
    pub round_duration_secs: u64,  // deadline length for each round, e.g. 28 days
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub addr: Address,
    pub active: bool,
    pub defaulted: bool,
    pub won_this_rotation: bool,
    pub contributed_this_round: bool,
    pub bid_bps: i32,           // -1 = no bid submitted yet this round
    pub total_wins: u32,
    pub total_contributed: i128,
    pub total_received: i128,   // net payouts + bonuses, for the member's own dashboard
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GroupState {
    pub config: GroupConfig,
    pub members: Vec<Member>,
    pub waitlist: Vec<Address>,          // FIFO queue, promoted in order
    pub locked: bool,
    pub round: u32,
    pub rotation: u32,
    pub reserve: i128,
    pub total_uncovered_shortfall: i128, // running bad-debt tally, always visible on-chain
    pub round_deadline: u64,             // ledger timestamp; past this, settle_round may auto-default
    pub open_slots: u32,                 // slots freed by default, fillable from the waitlist
}

#[contracttype]
pub enum DataKey {
    Group(Symbol),      // group id -> GroupState
    ProtocolAdmin,      // Address — controls the approved-token allowlist only,
                         // has no power over any individual group's funds
    ApprovedTokens,      // Vec<Address> — the stablecoin allowlist
    GroupRegistry,       // Vec<Symbol> — every group id ever created, for automation to enumerate
}

fn get_group(env: &Env, id: &Symbol) -> Result<GroupState, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Group(id.clone()))
        .ok_or(Error::GroupNotFound)
}

fn put_group(env: &Env, id: &Symbol, state: &GroupState) {
    env.storage()
        .persistent()
        .set(&DataKey::Group(id.clone()), state);
}

fn get_approved_tokens(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::ApprovedTokens)
        .unwrap_or(Vec::new(env))
}

fn is_token_approved(env: &Env, token: &Address) -> bool {
    get_approved_tokens(env).contains(token)
}

fn find_member_idx(members: &Vec<Member>, addr: &Address) -> Option<u32> {
    for i in 0..members.len() {
        if members.get(i).unwrap().addr == *addr {
            return Some(i);
        }
    }
    None
}

fn active_count(members: &Vec<Member>) -> u32 {
    let mut n = 0;
    for i in 0..members.len() {
        if members.get(i).unwrap().active {
            n += 1;
        }
    }
    n
}

/// Checks that every address in `supplied` is a distinct member of
/// `config.admins`, that each one actually authorized this call, and that
/// there are at least `config.admin_threshold` of them. This is the whole
/// multi-admin mechanism: N-of-M co-signature, checked atomically within
/// one transaction, rather than a stateful proposal/voting system.
fn require_admin_threshold(
    env: &Env,
    config: &GroupConfig,
    supplied: &Vec<Address>,
) -> Result<(), Error> {
    let mut seen: Vec<Address> = Vec::new(env);
    for i in 0..supplied.len() {
        let a = supplied.get(i).unwrap();
        if seen.contains(&a) {
            return Err(Error::DuplicateAdmin);
        }
        if !config.admins.contains(&a) {
            return Err(Error::NotAdmin);
        }
        a.require_auth();
        seen.push_back(a);
    }
    if seen.len() < config.admin_threshold {
        return Err(Error::NotEnoughAdminSignatures);
    }
    Ok(())
}

// ── Contract ────────────────────────────────────────────────────────────

#[contract]
pub struct LiholiswanoContractV2;

#[contractimpl]
impl LiholiswanoContractV2 {
    /// One-time setup: sets the protocol admin, the only address allowed to
    /// manage the stablecoin allowlist. This address has NO power over any
    /// individual group's funds, members, or settlements — it only controls
    /// which tokens `create_group` will accept. Call once, right after
    /// deployment.
    pub fn initialize(env: Env, protocol_admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::ProtocolAdmin) {
            return Err(Error::AlreadyInitialized);
        }
        protocol_admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ProtocolAdmin, &protocol_admin);
        env.storage()
            .instance()
            .set(&DataKey::ApprovedTokens, &Vec::<Address>::new(&env));
        env.storage()
            .persistent()
            .set(&DataKey::GroupRegistry, &Vec::<Symbol>::new(&env));
        Ok(())
    }

    /// Protocol-admin-only: add a token to the stablecoin allowlist. Every
    /// new group's `token` must already be on this list — settlements on
    /// this deployment only ever move an approved stablecoin, never an
    /// arbitrary or volatile asset.
    ///
    /// IMPORTANT — what this function cannot do: nothing on-chain can prove
    /// a token is genuinely a well-collateralized, 1:1-pegged stablecoin.
    /// This is a curated allowlist, not an automatic check — it is only as
    /// trustworthy as the protocol admin's own diligence in verifying each
    /// token's issuer before adding it (e.g. Circle's USDC contract ID,
    /// confirmed from Circle's own published address, not a search result).
    pub fn add_approved_token(env: Env, protocol_admin: Address, token: Address) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolAdmin)
            .ok_or(Error::NotInitialized)?;
        if protocol_admin != stored_admin {
            return Err(Error::NotProtocolAdmin);
        }
        protocol_admin.require_auth();

        let mut tokens = get_approved_tokens(&env);
        if !tokens.contains(&token) {
            tokens.push_back(token);
            env.storage().instance().set(&DataKey::ApprovedTokens, &tokens);
        }
        Ok(())
    }

    /// Protocol-admin-only: remove a token from the allowlist. Existing
    /// groups already using it are unaffected — this only blocks *new*
    /// groups from being created with it.
    pub fn remove_approved_token(env: Env, protocol_admin: Address, token: Address) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolAdmin)
            .ok_or(Error::NotInitialized)?;
        if protocol_admin != stored_admin {
            return Err(Error::NotProtocolAdmin);
        }
        protocol_admin.require_auth();

        let tokens = get_approved_tokens(&env);
        let mut filtered: Vec<Address> = Vec::new(&env);
        for i in 0..tokens.len() {
            let t = tokens.get(i).unwrap();
            if t != token {
                filtered.push_back(t);
            }
        }
        env.storage().instance().set(&DataKey::ApprovedTokens, &filtered);
        Ok(())
    }

    pub fn list_approved_tokens(env: Env) -> Vec<Address> {
        get_approved_tokens(&env)
    }

    /// Every group id ever created — lets an automation job (e.g. a
    /// scheduled GitHub Actions run) enumerate groups to call
    /// `settle_round` on, without needing its own off-chain database.
    pub fn list_groups(env: Env) -> Vec<Symbol> {
        env.storage()
            .persistent()
            .get(&DataKey::GroupRegistry)
            .unwrap_or(Vec::new(&env))
    }

    /// Create a new group. `id` must not already exist. Every address in
    /// `admins` must co-sign this call — you can't be made an admin
    /// without your own consent — and `admin_threshold` (1..=admins.len())
    /// sets how many of them must co-sign future admin actions.
    pub fn create_group(
        env: Env,
        id: Symbol,
        admins: Vec<Address>,
        admin_threshold: u32,
        token: Address,
        contribution: i128,
        collateral: i128,
        max_bid_bps: u32,
        max_members: u32,
        round_duration_secs: u64,
    ) -> Result<(), Error> {
        if env.storage().persistent().has(&DataKey::Group(id.clone())) {
            return Err(Error::GroupAlreadyExists);
        }
        if contribution <= 0
            || collateral < 0
            || max_bid_bps > 5000
            || max_members < 3
            || admins.len() == 0
            || admin_threshold == 0
            || admin_threshold > admins.len()
            || round_duration_secs == 0
        {
            return Err(Error::InvalidConfig);
        }
        if !env.storage().instance().has(&DataKey::ProtocolAdmin) {
            return Err(Error::NotInitialized);
        }
        if !is_token_approved(&env, &token) {
            return Err(Error::TokenNotApproved);
        }
        // Every proposed admin must consent by co-signing group creation.
        let mut seen: Vec<Address> = Vec::new(&env);
        for i in 0..admins.len() {
            let a = admins.get(i).unwrap();
            if seen.contains(&a) {
                return Err(Error::DuplicateAdmin);
            }
            a.require_auth();
            seen.push_back(a);
        }

        let state = GroupState {
            config: GroupConfig {
                admins,
                admin_threshold,
                token,
                contribution,
                collateral,
                max_bid_bps,
                max_members,
                round_duration_secs,
            },
            members: Vec::new(&env),
            waitlist: Vec::new(&env),
            locked: false,
            round: 0,
            rotation: 0,
            reserve: 0,
            total_uncovered_shortfall: 0,
            round_deadline: 0,
            open_slots: 0,
        };
        put_group(&env, &id, &state);

        let mut registry: Vec<Symbol> = env
            .storage()
            .persistent()
            .get(&DataKey::GroupRegistry)
            .unwrap_or(Vec::new(&env));
        registry.push_back(id);
        env.storage().persistent().set(&DataKey::GroupRegistry, &registry);

        Ok(())
    }

    /// Join an unlocked group, posting collateral via a real token transfer
    /// from the member to the contract.
    pub fn join_group(env: Env, id: Symbol, member: Address) -> Result<(), Error> {
        member.require_auth();
        let mut state = get_group(&env, &id)?;

        if state.locked {
            return Err(Error::AlreadyLocked);
        }
        if state.members.len() >= state.config.max_members {
            return Err(Error::GroupFull);
        }
        if find_member_idx(&state.members, &member).is_some() {
            return Err(Error::AlreadyMember);
        }

        if state.config.collateral > 0 {
            let token_client = token::Client::new(&env, &state.config.token);
            token_client.transfer(
                &member,
                &env.current_contract_address(),
                &state.config.collateral,
            );
        }

        state.members.push_back(Member {
            addr: member,
            active: true,
            defaulted: false,
            won_this_rotation: false,
            contributed_this_round: false,
            bid_bps: -1,
            total_wins: 0,
            total_contributed: 0,
            total_received: 0,
        });
        put_group(&env, &id, &state);
        Ok(())
    }

    /// Queue to join a group that's already locked or full. No collateral
    /// is taken yet — that only happens when actually promoted.
    pub fn join_waitlist(env: Env, id: Symbol, member: Address) -> Result<(), Error> {
        member.require_auth();
        let mut state = get_group(&env, &id)?;

        if find_member_idx(&state.members, &member).is_some() {
            return Err(Error::AlreadyMember);
        }
        if state.waitlist.contains(&member) {
            return Err(Error::AlreadyOnWaitlist);
        }
        state.waitlist.push_back(member);
        put_group(&env, &id, &state);
        Ok(())
    }

    /// Promote the member at the front of the waitlist into an open slot
    /// (one freed by an earlier default). Must be called by that exact
    /// member, in queue order, and posts collateral just like a normal
    /// join.
    pub fn promote_from_waitlist(env: Env, id: Symbol, member: Address) -> Result<(), Error> {
        member.require_auth();
        let mut state = get_group(&env, &id)?;

        if state.open_slots == 0 {
            return Err(Error::NoOpenSlots);
        }
        if state.waitlist.len() == 0 || state.waitlist.get(0).unwrap() != member {
            return Err(Error::NotNextInWaitlist);
        }
        if find_member_idx(&state.members, &member).is_some() {
            return Err(Error::AlreadyMember);
        }

        if state.config.collateral > 0 {
            let token_client = token::Client::new(&env, &state.config.token);
            token_client.transfer(
                &member,
                &env.current_contract_address(),
                &state.config.collateral,
            );
        }

        state.waitlist.remove(0);
        state.open_slots -= 1;
        state.members.push_back(Member {
            addr: member,
            active: true,
            defaulted: false,
            won_this_rotation: false,
            contributed_this_round: false,
            bid_bps: -1,
            total_wins: 0,
            total_contributed: 0,
            total_received: 0,
        });
        put_group(&env, &id, &state);
        Ok(())
    }

    /// Requires a threshold of admins to co-sign. Locks the group (no more
    /// direct joins — the waitlist takes over from here) and starts round
    /// 1, setting the first round's deadline.
    pub fn lock_group(env: Env, id: Symbol, admins: Vec<Address>) -> Result<(), Error> {
        let mut state = get_group(&env, &id)?;
        require_admin_threshold(&env, &state.config, &admins)?;

        if state.locked {
            return Err(Error::AlreadyLocked);
        }
        if state.members.len() < 3 {
            return Err(Error::TooFewMembersToLock);
        }

        state.locked = true;
        state.round = 1;
        state.rotation = 1;
        state.round_deadline = env.ledger().timestamp() + state.config.round_duration_secs;
        put_group(&env, &id, &state);
        Ok(())
    }

    /// A member pays this round's contribution via real token transfer.
    pub fn contribute(env: Env, id: Symbol, member: Address) -> Result<(), Error> {
        member.require_auth();
        let mut state = get_group(&env, &id)?;

        if !state.locked {
            return Err(Error::NotLocked);
        }
        let idx = find_member_idx(&state.members, &member).ok_or(Error::NotAMember)?;
        let mut m = state.members.get(idx).unwrap();
        if !m.active {
            return Err(Error::MemberNotActive);
        }
        if m.contributed_this_round {
            return Err(Error::AlreadyContributedThisRound);
        }

        let token_client = token::Client::new(&env, &state.config.token);
        token_client.transfer(
            &member,
            &env.current_contract_address(),
            &state.config.contribution,
        );

        m.contributed_this_round = true;
        m.total_contributed += state.config.contribution;
        state.members.set(idx, m);
        put_group(&env, &id, &state);
        Ok(())
    }

    /// A member submits their compulsory bid, in basis points of the pot
    /// (0 is a valid, and common, bid). Only eligible members (active, not
    /// already won this rotation) may bid.
    pub fn submit_bid(env: Env, id: Symbol, member: Address, bid_bps: u32) -> Result<(), Error> {
        member.require_auth();
        let mut state = get_group(&env, &id)?;

        if !state.locked {
            return Err(Error::NotLocked);
        }
        if bid_bps > state.config.max_bid_bps {
            return Err(Error::BidTooHigh);
        }
        let idx = find_member_idx(&state.members, &member).ok_or(Error::NotAMember)?;
        let mut m = state.members.get(idx).unwrap();
        if !m.active {
            return Err(Error::MemberNotActive);
        }
        if m.won_this_rotation {
            return Err(Error::NotEligibleThisRotation);
        }
        if m.bid_bps >= 0 {
            return Err(Error::AlreadyBidThisRound);
        }

        m.bid_bps = bid_bps as i32;
        state.members.set(idx, m);
        put_group(&env, &id, &state);
        Ok(())
    }

    /// Settle the round. Two ways this can succeed:
    ///
    /// 1. Before the deadline: only once every active member has
    ///    contributed and every eligible member has bid — same as the
    ///    first pilot, unchanged.
    /// 2. At or after the deadline: anyone may call this even if some
    ///    active members never contributed or bid. Those stragglers are
    ///    automatically defaulted first (collateral seized into the
    ///    reserve, a slot opened for the waitlist, and — if they'd already
    ///    won this rotation — their remaining obligation is covered by
    ///    that collateral with any true gap logged as bad debt, exactly
    ///    like the manual default path). Any active member who contributed
    ///    but never bid is treated as having bid 0 (no rush) rather than
    ///    blocking the round. Settlement then proceeds among whoever is
    ///    left active.
    ///
    /// The rest of the payout math (highest-bid wins, floor-split bonus
    /// with the remainder to reserve) is identical to the first pilot.
    pub fn settle_round(env: Env, id: Symbol) -> Result<(), Error> {
        let mut state = get_group(&env, &id)?;
        if !state.locked {
            return Err(Error::NotLocked);
        }

        let now = env.ledger().timestamp();
        let deadline_passed = now >= state.round_deadline;

        // Gate: before the deadline, everyone must have acted. At/after
        // the deadline, auto-resolve stragglers instead of blocking.
        let mut i = 0;
        while i < state.members.len() {
            let m = state.members.get(i).unwrap();
            if m.active && !m.contributed_this_round {
                if !deadline_passed {
                    return Err(Error::NotAllContributed);
                }
                auto_default_member(&env, &mut state, i);
                // Don't advance i — the member at this index changed.
                continue;
            }
            i += 1;
        }

        let n_active = active_count(&state.members);
        if n_active == 0 {
            put_group(&env, &id, &state);
            return Err(Error::NothingToSettle);
        }

        let mut eligible_idxs: Vec<u32> = Vec::new(&env);
        for i in 0..state.members.len() {
            let mut m = state.members.get(i).unwrap();
            if m.active && !m.won_this_rotation {
                if m.bid_bps < 0 {
                    if !deadline_passed {
                        return Err(Error::NotAllBid);
                    }
                    // No bid by the deadline = treated as a 0% bid.
                    m.bid_bps = 0;
                    state.members.set(i, m.clone());
                }
                eligible_idxs.push_back(i);
            }
        }

        // If nobody was eligible, the rotation just completed — reset and
        // treat every active member as freshly eligible for the new one.
        if eligible_idxs.len() == 0 {
            for i in 0..state.members.len() {
                let mut m = state.members.get(i).unwrap();
                if m.active {
                    m.won_this_rotation = false;
                    state.members.set(i, m);
                }
            }
            state.rotation += 1;
            for i in 0..state.members.len() {
                let mut m = state.members.get(i).unwrap();
                if m.active {
                    if m.bid_bps < 0 {
                        if !deadline_passed {
                            return Err(Error::NotAllBid);
                        }
                        m.bid_bps = 0;
                        state.members.set(i, m.clone());
                    }
                    eligible_idxs.push_back(i);
                }
            }
        }

        // n_active may have dropped if the reset loop above ran after more
        // auto-defaults were already applied; recompute for the payout math.
        let n_active = active_count(&state.members);
        if n_active == 0 {
            put_group(&env, &id, &state);
            return Err(Error::NothingToSettle);
        }

        // Winner = highest bid; ties broken by lowest member index (stable,
        // matches the simulator's tie-break rule so results are comparable).
        let mut winner_pos = eligible_idxs.get(0).unwrap();
        let mut winner_bid = state.members.get(winner_pos).unwrap().bid_bps;
        for k in 1..eligible_idxs.len() {
            let idx = eligible_idxs.get(k).unwrap();
            let bid = state.members.get(idx).unwrap().bid_bps;
            if bid > winner_bid {
                winner_bid = bid;
                winner_pos = idx;
            }
        }

        let pot: i128 = (n_active as i128) * state.config.contribution;
        let bid_amount: i128 = (pot * (winner_bid as i128)) / 10_000;
        let net_to_winner: i128 = pot - bid_amount;

        let others_count: i128 = (n_active as i128) - 1;
        let share: i128 = if others_count > 0 { bid_amount / others_count } else { 0 };
        let remainder: i128 = bid_amount - share * others_count;

        let token_client = token::Client::new(&env, &state.config.token);
        let winner_addr = state.members.get(winner_pos).unwrap().addr.clone();
        if net_to_winner > 0 {
            token_client.transfer(&env.current_contract_address(), &winner_addr, &net_to_winner);
        }

        for i in 0..state.members.len() {
            let mut m = state.members.get(i).unwrap();
            if !m.active {
                continue;
            }
            if i == winner_pos {
                m.won_this_rotation = true;
                m.total_wins += 1;
                m.total_received += net_to_winner;
            } else if share > 0 {
                token_client.transfer(&env.current_contract_address(), &m.addr, &share);
                m.total_received += share;
            }
            // Reset per-round flags for the next round.
            m.contributed_this_round = false;
            m.bid_bps = -1;
            state.members.set(i, m);
        }

        state.reserve += remainder;
        state.round += 1;
        state.round_deadline = now + state.config.round_duration_secs;
        put_group(&env, &id, &state);
        Ok(())
    }

    /// Requires a threshold of admins to co-sign. Manually marks a member
    /// as defaulted (e.g. for an obvious problem before the deadline, or
    /// as a fallback). Identical accounting to the automatic path inside
    /// `settle_round`, and also opens a slot for the waitlist.
    pub fn mark_default(
        env: Env,
        id: Symbol,
        admins: Vec<Address>,
        member: Address,
    ) -> Result<(), Error> {
        let mut state = get_group(&env, &id)?;
        require_admin_threshold(&env, &state.config, &admins)?;

        let idx = find_member_idx(&state.members, &member).ok_or(Error::NotAMember)?;
        let m = state.members.get(idx).unwrap();
        if m.defaulted {
            return Err(Error::MemberAlreadyDefaulted);
        }

        auto_default_member(&env, &mut state, idx);
        put_group(&env, &id, &state);
        Ok(())
    }

    // ── Read-only views ───────────────────────────────────────────────

    pub fn get_group_state(env: Env, id: Symbol) -> Result<GroupState, Error> {
        get_group(&env, &id)
    }

    pub fn get_member(env: Env, id: Symbol, member: Address) -> Result<Member, Error> {
        let state = get_group(&env, &id)?;
        let idx = find_member_idx(&state.members, &member).ok_or(Error::NotAMember)?;
        Ok(state.members.get(idx).unwrap())
    }
}

/// Shared default logic for both the automatic (deadline-triggered) and
/// manual (admin-triggered) paths: seizes collateral into the reserve,
/// covers as much of any still-owed future contributions as that
/// collateral allows (logging only the genuine gap as bad debt, never
/// fabricating it), marks the member inactive, and opens a slot for the
/// waitlist. Mutates `state` in place; the caller is responsible for
/// persisting it.
fn auto_default_member(env: &Env, state: &mut GroupState, idx: u32) {
    let mut m = state.members.get(idx).unwrap();
    let collateral = state.config.collateral;
    let mut uncovered: i128 = 0;

    if m.won_this_rotation {
        let mut still_to_win: i128 = 0;
        for i in 0..state.members.len() {
            let other = state.members.get(i).unwrap();
            if other.active && !other.won_this_rotation && other.addr != m.addr {
                still_to_win += 1;
            }
        }
        let owed = still_to_win * state.config.contribution;
        let covered = if owed < collateral { owed } else { collateral };
        uncovered = owed - covered;
    }

    m.active = false;
    m.defaulted = true;
    state.members.set(idx, m);
    state.reserve += collateral;
    state.total_uncovered_shortfall += uncovered;
    state.open_slots += 1;
    let _ = env; // reserved for future use (e.g. events); keeps the signature stable
}

mod test;
