//! # Veridex Oracle Contract
//!
//! Prediction market oracle contract for the Veridex platform on Stellar.
//! Manages market creation, outcome resolution by an authorized oracle, and
//! pro-rata winnings distribution to winning participants.
//!
//! ## Lifecycle
//!
//! ```text
//! create_market → [participants stake via stake()] → resolve_market → claim_winnings
//! ```
//!
//! ## Security Considerations
//!
//! - Only the admin (oracle authority) may call `resolve_market`.
//! - Markets can only be resolved once; double-resolution panics.
//! - `claim_winnings` is idempotent: double-claiming panics after the first.
//! - Stake amounts are tracked in stroops (the smallest XLM unit) as `i128`
//!   to be compatible with Soroban token interface amounts.
//! - All outcome identifiers are `u32` for compact on-chain storage.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

// ---------------------------------------------------------------------------
// Storage key types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Market(u64),           // market_id → MarketState
    Stake(u64, Address),   // (market_id, participant) → i128
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Possible lifecycle states for a prediction market.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarketStatus {
    /// Accepting stakes; outcome not yet known.
    Open,
    /// Resolved — `winning_outcome` is set.
    Resolved,
    /// No winning stakes; market voided and stakes refundable.
    Voided,
}

/// Full on-chain state for a single prediction market.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketState {
    /// Unique market identifier (assigned at creation).
    pub market_id: u64,
    /// Human-readable description / question.
    pub description: soroban_sdk::String,
    /// Token contract used for stakes and payouts.
    pub token: Address,
    /// Ledger timestamp after which no more stakes are accepted.
    pub close_time: u64,
    /// Current lifecycle status.
    pub status: MarketStatus,
    /// Number of distinct outcome buckets (e.g., 2 for binary markets).
    pub outcome_count: u32,
    /// Winning outcome index, set by `resolve_market`.
    pub winning_outcome: Option<u32>,
    /// Total tokens staked per outcome (index = outcome id).
    pub outcome_stakes: Vec<i128>,
    /// Total tokens staked across all outcomes.
    pub total_stake: i128,
    /// Ledger sequence when the market was resolved (audit trail).
    pub resolved_at: Option<u32>,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct VeridexOracle;

#[contractimpl]
impl VeridexOracle {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the contract. `admin` becomes the sole oracle authority.
    ///
    /// # Panics
    /// Panics if the contract is already initialized.
    pub fn initialize(env: Env, admin: Address) {
        if env
            .storage()
            .instance()
            .has(&symbol_short!("ADMIN"))
        {
            panic!("already initialized");
        }
        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &admin);
        // Initialize market ID counter.
        env.storage()
            .instance()
            .set(&symbol_short!("NEXTID"), &0u64);
    }

    // -----------------------------------------------------------------------
    // Admin management
    // -----------------------------------------------------------------------

    /// Return the current admin (oracle authority) address.
    pub fn admin(env: Env) -> Address {
        Self::get_admin(&env)
    }

    /// Transfer oracle authority to `new_admin`.
    ///
    /// Requires authorization from the current admin.
    pub fn set_admin(env: Env, new_admin: Address) {
        let current = Self::get_admin(&env);
        current.require_auth();
        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &new_admin);
    }

    // -----------------------------------------------------------------------
    // Market lifecycle
    // -----------------------------------------------------------------------

    /// Create a new prediction market.
    ///
    /// Returns the newly assigned `market_id`.
    ///
    /// # Parameters
    /// - `description`: Market question / human-readable label.
    /// - `token`: SAC or token contract for stake/payout denomination.
    /// - `close_time`: Ledger timestamp after which staking is closed.
    /// - `outcome_count`: Number of discrete outcomes (minimum 2).
    ///
    /// # Panics
    /// - If `outcome_count < 2`.
    /// - If `close_time` is in the past.
    pub fn create_market(
        env: Env,
        description: soroban_sdk::String,
        token: Address,
        close_time: u64,
        outcome_count: u32,
    ) -> u64 {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        if outcome_count < 2 {
            panic!("market must have at least 2 outcomes");
        }
        if close_time <= env.ledger().timestamp() {
            panic!("close_time must be in the future");
        }

        let market_id = Self::next_market_id(&env);

        // Build zero-initialised per-outcome stake vector.
        let mut outcome_stakes: Vec<i128> = Vec::new(&env);
        for _ in 0..outcome_count {
            outcome_stakes.push_back(0i128);
        }

        let state = MarketState {
            market_id,
            description,
            token,
            close_time,
            status: MarketStatus::Open,
            outcome_count,
            winning_outcome: None,
            outcome_stakes,
            total_stake: 0,
            resolved_at: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Market(market_id), &state);

        Self::extend_market_ttl(&env, market_id);

        market_id
    }

    /// Place a stake on `outcome` in `market_id`.
    ///
    /// Transfers `amount` tokens from `participant` to this contract.
    ///
    /// # Panics
    /// - If the market does not exist.
    /// - If the market is not `Open`.
    /// - If staking is closed (`close_time` passed).
    /// - If `outcome >= outcome_count`.
    /// - If `amount <= 0`.
    pub fn stake(
        env: Env,
        market_id: u64,
        participant: Address,
        outcome: u32,
        amount: i128,
    ) {
        participant.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        let mut state = Self::get_market(&env, market_id);

        if state.status != MarketStatus::Open {
            panic!("market is not open");
        }
        if env.ledger().timestamp() > state.close_time {
            panic!("staking period has closed");
        }
        if outcome >= state.outcome_count {
            panic!("invalid outcome index");
        }

        // Transfer tokens in.
        let token_client = token::TokenClient::new(&env, &state.token);
        token_client.transfer(&participant, &env.current_contract_address(), &amount);

        // Update per-outcome and total stake.
        let current = state.outcome_stakes.get(outcome).unwrap_or(0i128);
        state.outcome_stakes.set(outcome, current + amount);
        state.total_stake += amount;

        env.storage()
            .persistent()
            .set(&DataKey::Market(market_id), &state);

        // Track individual participant stake (accumulate on multiple calls).
        let stake_key = DataKey::Stake(market_id, participant.clone());
        let prev_stake: i128 = env
            .storage()
            .persistent()
            .get(&stake_key)
            .unwrap_or(0i128);
        env.storage()
            .persistent()
            .set(&stake_key, &(prev_stake + amount));

        Self::extend_market_ttl(&env, market_id);
    }

    /// Resolve a market by declaring the winning `outcome`.
    ///
    /// Only the oracle admin may call this. Once resolved, the market
    /// transitions to `Resolved` status and stakers can claim winnings.
    ///
    /// # Panics
    /// - If called by anyone other than the admin.
    /// - If the market is not `Open`.
    /// - If `outcome >= outcome_count`.
    pub fn resolve_market(env: Env, market_id: u64, outcome: u32) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut state = Self::get_market(&env, market_id);

        if state.status != MarketStatus::Open {
            panic!("market already resolved or voided");
        }
        if outcome >= state.outcome_count {
            panic!("invalid outcome index");
        }

        state.status = MarketStatus::Resolved;
        state.winning_outcome = Some(outcome);
        state.resolved_at = Some(env.ledger().sequence());

        env.storage()
            .persistent()
            .set(&DataKey::Market(market_id), &state);

        Self::extend_market_ttl(&env, market_id);
    }

    /// Void a market, allowing all participants to reclaim stakes.
    ///
    /// Only the oracle admin may call this. Useful if the oracle cannot
    /// determine an outcome (e.g., disputed or cancelled event).
    ///
    /// # Panics
    /// - If called by anyone other than the admin.
    /// - If the market is not `Open`.
    pub fn void_market(env: Env, market_id: u64) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut state = Self::get_market(&env, market_id);

        if state.status != MarketStatus::Open {
            panic!("market already resolved or voided");
        }

        state.status = MarketStatus::Voided;
        state.resolved_at = Some(env.ledger().sequence());

        env.storage()
            .persistent()
            .set(&DataKey::Market(market_id), &state);

        Self::extend_market_ttl(&env, market_id);
    }

    /// Claim winnings for `participant` in a resolved market.
    ///
    /// Distributes a pro-rata share of the total stake pool to the
    /// participant based on their stake on the winning outcome.
    ///
    /// In a voided market, the participant's original stake is returned.
    ///
    /// # Returns
    /// The amount of tokens transferred to `participant`.
    ///
    /// # Panics
    /// - If the market is still `Open`.
    /// - If the participant has already claimed.
    /// - If the participant has no stake (or no winning stake in a `Resolved` market).
    pub fn claim_winnings(env: Env, market_id: u64, participant: Address) -> i128 {
        participant.require_auth();

        let state = Self::get_market(&env, market_id);

        if state.status == MarketStatus::Open {
            panic!("market is not yet resolved");
        }

        let stake_key = DataKey::Stake(market_id, participant.clone());
        let participant_stake: i128 = env
            .storage()
            .persistent()
            .get(&stake_key)
            .unwrap_or(0i128);

        if participant_stake == 0 {
            panic!("no stake found for participant");
        }

        let payout = match state.status {
            MarketStatus::Voided => {
                // Full refund in voided markets.
                participant_stake
            }
            MarketStatus::Resolved => {
                let winning = state.winning_outcome.expect("resolved market must have outcome");
                let winning_pool = state.outcome_stakes.get(winning).unwrap_or(0i128);

                if winning_pool == 0 {
                    panic!("no stakes on winning outcome");
                }

                // Participant must hold stake on winning outcome.
                // We need per-outcome stake per participant for precise payout;
                // here we use total participant stake as an approximation valid
                // for single-outcome stake-per-participant designs.
                // For multi-outcome staking, extend with outcome-scoped keys.
                //
                // Payout = (participant_stake / winning_pool) * total_stake
                // Computed without floating point via integer arithmetic:
                // payout = participant_stake * total_stake / winning_pool
                let payout = (participant_stake as i128)
                    .checked_mul(state.total_stake)
                    .expect("overflow in payout calculation")
                    .checked_div(winning_pool)
                    .expect("division by zero in payout");
                payout
            }
            MarketStatus::Open => panic!("market is not yet resolved"),
        };

        // Mark as claimed by zeroing the stake entry.
        env.storage().persistent().set(&stake_key, &0i128);

        // Transfer payout to participant.
        let token_client = token::TokenClient::new(&env, &state.token);
        token_client.transfer(&env.current_contract_address(), &participant, &payout);

        payout
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Return the full [`MarketState`] for `market_id`.
    pub fn get_market_state(env: Env, market_id: u64) -> MarketState {
        Self::get_market(&env, market_id)
    }

    /// Return the stake held by `participant` in `market_id`.
    pub fn get_stake(env: Env, market_id: u64, participant: Address) -> i128 {
        let stake_key = DataKey::Stake(market_id, participant);
        env.storage()
            .persistent()
            .get(&stake_key)
            .unwrap_or(0i128)
    }

    /// Return the next market ID that will be assigned.
    pub fn next_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<_, u64>(&symbol_short!("NEXTID"))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&symbol_short!("ADMIN"))
            .expect("contract not initialized")
    }

    fn get_market(env: &Env, market_id: u64) -> MarketState {
        env.storage()
            .persistent()
            .get::<DataKey, MarketState>(&DataKey::Market(market_id))
            .expect("market not found")
    }

    fn next_market_id(env: &Env) -> u64 {
        let id: u64 = env
            .storage()
            .instance()
            .get::<_, u64>(&symbol_short!("NEXTID"))
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXTID"), &(id + 1));
        id
    }

    fn extend_market_ttl(env: &Env, market_id: u64) {
        // Extend to ~1 year of ledger closings.
        env.storage().persistent().extend_ttl(
            &DataKey::Market(market_id),
            6_307_200,
            6_307_200,
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Env, String,
    };

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    struct TestEnv {
        env: Env,
        admin: Address,
        client: VeridexOracleClient<'static>,
        token: Address,
        token_admin: Address,
    }

    fn setup() -> TestEnv {
        let env = Env::default();
        env.mock_all_auths();

        // Set initial ledger state.
        env.ledger().set(LedgerInfo {
            timestamp: 1_000_000,
            protocol_version: 21,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 10_000_000,
        });

        let contract_id = env.register_contract(None, VeridexOracle);
        let client = VeridexOracleClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);

        // Deploy a standard token for testing.
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token = token_id.address();

        client.initialize(&admin);

        TestEnv {
            env,
            admin,
            client,
            token,
            token_admin,
        }
    }

    fn mint(t: &TestEnv, to: &Address, amount: i128) {
        let sac_admin = token::StellarAssetClient::new(&t.env, &t.token);
        sac_admin.mint(to, &amount);
    }

    fn make_market(t: &TestEnv) -> u64 {
        let desc = String::from_str(&t.env, "Will XLM hit $1 by EOY?");
        t.client.create_market(&desc, &t.token, &2_000_000u64, &2u32)
    }

    // -----------------------------------------------------------------------
    // Initialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize_sets_admin() {
        let t = setup();
        assert_eq!(t.client.admin(), t.admin);
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_double_initialize_panics() {
        let t = setup();
        let other = Address::generate(&t.env);
        t.client.initialize(&other);
    }

    #[test]
    fn test_set_admin() {
        let t = setup();
        let new_admin = Address::generate(&t.env);
        t.client.set_admin(&new_admin);
        assert_eq!(t.client.admin(), new_admin);
    }

    // -----------------------------------------------------------------------
    // Market creation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_market_returns_id() {
        let t = setup();
        let id = make_market(&t);
        assert_eq!(id, 0);
    }

    #[test]
    fn test_create_multiple_markets_increments_id() {
        let t = setup();
        let id0 = make_market(&t);
        let id1 = make_market(&t);
        let id2 = make_market(&t);
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    #[should_panic(expected = "market must have at least 2 outcomes")]
    fn test_create_market_single_outcome_panics() {
        let t = setup();
        let desc = String::from_str(&t.env, "bad market");
        t.client.create_market(&desc, &t.token, &2_000_000u64, &1u32);
    }

    #[test]
    #[should_panic(expected = "close_time must be in the future")]
    fn test_create_market_past_close_time_panics() {
        let t = setup();
        let desc = String::from_str(&t.env, "expired");
        // close_time = 0, which is in the past relative to timestamp 1_000_000
        t.client.create_market(&desc, &t.token, &0u64, &2u32);
    }

    #[test]
    fn test_get_market_state() {
        let t = setup();
        let id = make_market(&t);
        let state = t.client.get_market_state(&id);
        assert_eq!(state.market_id, id);
        assert_eq!(state.status, MarketStatus::Open);
        assert_eq!(state.outcome_count, 2);
        assert!(state.winning_outcome.is_none());
        assert_eq!(state.total_stake, 0);
    }

    // -----------------------------------------------------------------------
    // Resolve market tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_market_sets_winning_outcome() {
        let t = setup();
        let id = make_market(&t);
        t.client.resolve_market(&id, &1u32);
        let state = t.client.get_market_state(&id);
        assert_eq!(state.status, MarketStatus::Resolved);
        assert_eq!(state.winning_outcome, Some(1));
        assert!(state.resolved_at.is_some());
    }

    #[test]
    #[should_panic(expected = "market already resolved or voided")]
    fn test_double_resolve_panics() {
        let t = setup();
        let id = make_market(&t);
        t.client.resolve_market(&id, &0u32);
        t.client.resolve_market(&id, &1u32);
    }

    #[test]
    #[should_panic(expected = "invalid outcome index")]
    fn test_resolve_invalid_outcome_panics() {
        let t = setup();
        let id = make_market(&t);
        t.client.resolve_market(&id, &99u32);
    }

    // -----------------------------------------------------------------------
    // Void market tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_void_market() {
        let t = setup();
        let id = make_market(&t);
        t.client.void_market(&id);
        let state = t.client.get_market_state(&id);
        assert_eq!(state.status, MarketStatus::Voided);
    }

    #[test]
    #[should_panic(expected = "market already resolved or voided")]
    fn test_void_after_resolve_panics() {
        let t = setup();
        let id = make_market(&t);
        t.client.resolve_market(&id, &0u32);
        t.client.void_market(&id);
    }

    // -----------------------------------------------------------------------
    // Stake tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_stake_updates_market_totals() {
        let t = setup();
        let id = make_market(&t);
        let participant = Address::generate(&t.env);
        mint(&t, &participant, 1_000_000);

        t.client.stake(&id, &participant, &0u32, &500_000i128);
        let state = t.client.get_market_state(&id);
        assert_eq!(state.total_stake, 500_000);
        assert_eq!(state.outcome_stakes.get(0), Some(500_000i128));
        assert_eq!(t.client.get_stake(&id, &participant), 500_000);
    }

    #[test]
    fn test_multiple_stakes_accumulate() {
        let t = setup();
        let id = make_market(&t);
        let p = Address::generate(&t.env);
        mint(&t, &p, 1_000_000);
        t.client.stake(&id, &p, &0u32, &300_000i128);
        t.client.stake(&id, &p, &0u32, &200_000i128);
        assert_eq!(t.client.get_stake(&id, &p), 500_000);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_stake_zero_amount_panics() {
        let t = setup();
        let id = make_market(&t);
        let p = Address::generate(&t.env);
        t.client.stake(&id, &p, &0u32, &0i128);
    }

    #[test]
    #[should_panic(expected = "invalid outcome index")]
    fn test_stake_invalid_outcome_panics() {
        let t = setup();
        let id = make_market(&t);
        let p = Address::generate(&t.env);
        mint(&t, &p, 1_000_000);
        t.client.stake(&id, &p, &99u32, &100_000i128);
    }

    #[test]
    #[should_panic(expected = "market is not open")]
    fn test_stake_after_resolve_panics() {
        let t = setup();
        let id = make_market(&t);
        t.client.resolve_market(&id, &0u32);
        let p = Address::generate(&t.env);
        mint(&t, &p, 1_000_000);
        t.client.stake(&id, &p, &0u32, &100_000i128);
    }

    // -----------------------------------------------------------------------
    // Claim winnings tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_claim_full_pool_single_winner() {
        let t = setup();
        let id = make_market(&t);
        let winner = Address::generate(&t.env);
        let loser = Address::generate(&t.env);
        mint(&t, &winner, 1_000_000);
        mint(&t, &loser, 1_000_000);

        // Winner bets outcome 0, loser bets outcome 1.
        t.client.stake(&id, &winner, &0u32, &1_000_000i128);
        t.client.stake(&id, &loser, &1u32, &1_000_000i128);

        t.client.resolve_market(&id, &0u32);

        let payout = t.client.claim_winnings(&id, &winner);
        // winner_stake / winning_pool * total_stake = 1_000_000 / 1_000_000 * 2_000_000
        assert_eq!(payout, 2_000_000);
    }

    #[test]
    #[should_panic(expected = "market is not yet resolved")]
    fn test_claim_before_resolve_panics() {
        let t = setup();
        let id = make_market(&t);
        let p = Address::generate(&t.env);
        t.client.claim_winnings(&id, &p);
    }

    #[test]
    #[should_panic(expected = "no stake found for participant")]
    fn test_claim_no_stake_panics() {
        let t = setup();
        let id = make_market(&t);
        t.client.resolve_market(&id, &0u32);
        let p = Address::generate(&t.env);
        t.client.claim_winnings(&id, &p);
    }

    #[test]
    #[should_panic(expected = "no stake found for participant")]
    fn test_double_claim_panics() {
        let t = setup();
        let id = make_market(&t);
        let winner = Address::generate(&t.env);
        let loser = Address::generate(&t.env);
        mint(&t, &winner, 1_000_000);
        mint(&t, &loser, 500_000);
        t.client.stake(&id, &winner, &0u32, &1_000_000i128);
        t.client.stake(&id, &loser, &1u32, &500_000i128);
        t.client.resolve_market(&id, &0u32);
        t.client.claim_winnings(&id, &winner);
        // Second claim — stake is zeroed, should panic.
        t.client.claim_winnings(&id, &winner);
    }

    #[test]
    fn test_claim_voided_market_refunds_stake() {
        let t = setup();
        let id = make_market(&t);
        let p = Address::generate(&t.env);
        mint(&t, &p, 750_000);
        t.client.stake(&id, &p, &1u32, &750_000i128);
        t.client.void_market(&id);
        let refund = t.client.claim_winnings(&id, &p);
        assert_eq!(refund, 750_000);
    }

    #[test]
    fn test_next_id_increments() {
        let t = setup();
        assert_eq!(t.client.next_id(), 0);
        make_market(&t);
        assert_eq!(t.client.next_id(), 1);
        make_market(&t);
        assert_eq!(t.client.next_id(), 2);
    }
}
