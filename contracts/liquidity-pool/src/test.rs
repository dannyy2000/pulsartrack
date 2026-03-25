#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};

fn deploy_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}
fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn setup(env: &Env) -> (LiquidityPoolContractClient<'_>, Address, Address, Address) {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = deploy_token(env, &token_admin);
    let contract_id = env.register_contract(None, LiquidityPoolContract);
    let c = LiquidityPoolContractClient::new(env, &contract_id);
    c.initialize(&admin, &token);
    (c, admin, token_admin, token)
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    setup(&env);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, admin, _, token) = setup(&env);
    c.initialize(&admin, &token);
}

#[test]
fn test_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    mint(&env, &token, &provider, 1_000_000);
    let shares = c.deposit(&provider, &100_000i128);
    assert!(shares > 0);
    let pos = c.get_provider_position(&provider).unwrap();
    assert_eq!(pos.shares, shares);
    let pool = c.get_pool_state();
    assert_eq!(pool.total_liquidity, 100_000);
}

#[test]
fn test_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    mint(&env, &token, &provider, 1_000_000);
    let shares = c.deposit(&provider, &100_000i128);
    let withdrawn = c.withdraw(&provider, &shares);
    assert_eq!(withdrawn, 100_000);
    let pool = c.get_pool_state();
    assert_eq!(pool.total_liquidity, 0);
}

#[test]
fn test_borrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &token, &provider, 1_000_000);
    c.deposit(&provider, &500_000i128);
    c.borrow(&borrower, &1u64, &100_000i128, &86_400u64);
    let borrow = c.get_borrow(&1u64).unwrap();
    assert_eq!(borrow.borrowed, 100_000);
    let pool = c.get_pool_state();
    assert_eq!(pool.total_borrowed, 100_000);
}

#[test]
fn test_repay() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &token, &provider, 1_000_000);
    mint(&env, &token, &borrower, 1_000_000);
    c.deposit(&provider, &500_000i128);
    c.borrow(&borrower, &1u64, &100_000i128, &86_400u64);
    c.repay(&borrower, &1u64, &100_000i128);
    let pool = c.get_pool_state();
    assert_eq!(pool.total_borrowed, 0);
}

#[test]
fn test_get_provider_position_nonexistent() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, _) = setup(&env);
    assert!(c.get_provider_position(&Address::generate(&env)).is_none());
}

#[test]
fn test_get_borrow_nonexistent() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, _) = setup(&env);
    assert!(c.get_borrow(&999u64).is_none());
}

#[test]
fn test_repay_with_interest_no_inflation() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    let borrower = Address::generate(&env);
    
    mint(&env, &token, &provider, 1_000_000);
    mint(&env, &token, &borrower, 1_000_000);
    
    // Provider deposits 500,000
    c.deposit(&provider, &500_000i128);
    let pool_after_deposit = c.get_pool_state();
    assert_eq!(pool_after_deposit.total_liquidity, 500_000);
    
    // Borrower borrows 100,000
    c.borrow(&borrower, &1u64, &100_000i128, &86_400u64);
    let pool_after_borrow = c.get_pool_state();
    assert_eq!(pool_after_borrow.total_liquidity, 500_000);
    assert_eq!(pool_after_borrow.total_borrowed, 100_000);
    
    // Borrower repays 110,000 (100,000 principal + 10,000 interest)
    c.repay(&borrower, &1u64, &110_000i128);
    let pool_after_repay = c.get_pool_state();
    
    // total_liquidity should only increase by principal (100,000), not full amount
    assert_eq!(pool_after_repay.total_liquidity, 500_000); // 500,000 original
    assert_eq!(pool_after_repay.total_borrowed, 0);
    assert_eq!(pool_after_repay.interest_reserve, 10_000); // Interest tracked separately
}

#[test]
fn test_repay_principal_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    let borrower = Address::generate(&env);
    
    mint(&env, &token, &provider, 1_000_000);
    mint(&env, &token, &borrower, 1_000_000);
    
    c.deposit(&provider, &500_000i128);
    c.borrow(&borrower, &1u64, &100_000i128, &86_400u64);
    
    // Repay exactly the principal
    c.repay(&borrower, &1u64, &100_000i128);
    let pool = c.get_pool_state();
    
    assert_eq!(pool.total_liquidity, 500_000);
    assert_eq!(pool.total_borrowed, 0);
    assert_eq!(pool.interest_reserve, 0); // No interest paid
}

#[test]
fn test_multiple_borrows_with_interest() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);
    
    mint(&env, &token, &provider, 1_000_000);
    mint(&env, &token, &borrower1, 1_000_000);
    mint(&env, &token, &borrower2, 1_000_000);
    
    // Provider deposits 500,000
    c.deposit(&provider, &500_000i128);
    
    // Two borrowers borrow
    c.borrow(&borrower1, &1u64, &100_000i128, &86_400u64);
    c.borrow(&borrower2, &2u64, &150_000i128, &86_400u64);
    
    let pool_after_borrows = c.get_pool_state();
    assert_eq!(pool_after_borrows.total_liquidity, 500_000);
    assert_eq!(pool_after_borrows.total_borrowed, 250_000);
    
    // First borrower repays with interest
    c.repay(&borrower1, &1u64, &105_000i128); // 100k + 5k interest
    let pool_after_first = c.get_pool_state();
    assert_eq!(pool_after_first.total_liquidity, 500_000);
    assert_eq!(pool_after_first.total_borrowed, 150_000);
    assert_eq!(pool_after_first.interest_reserve, 5_000);
    
    // Second borrower repays with interest
    c.repay(&borrower2, &2u64, &160_000i128); // 150k + 10k interest
    let pool_after_second = c.get_pool_state();
    assert_eq!(pool_after_second.total_liquidity, 500_000);
    assert_eq!(pool_after_second.total_borrowed, 0);
    assert_eq!(pool_after_second.interest_reserve, 15_000); // 5k + 10k
}

#[test]
fn test_provider_shares_not_inflated_by_interest() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    let borrower = Address::generate(&env);
    
    mint(&env, &token, &provider, 1_000_000);
    mint(&env, &token, &borrower, 1_000_000);
    
    // Provider deposits and gets shares
    let shares = c.deposit(&provider, &500_000i128);
    
    // Borrower borrows and repays with interest
    c.borrow(&borrower, &1u64, &100_000i128, &86_400u64);
    c.repay(&borrower, &1u64, &120_000i128); // 100k + 20k interest
    
    // Provider's shares should still be worth the same as original deposit
    let pool = c.get_pool_state();
    let position = c.get_provider_position(&provider).unwrap();
    
    // Share value calculation: (shares * total_liquidity) / total_shares
    // Should equal original deposit, not inflated by interest
    assert_eq!(position.shares, shares);
    assert_eq!(pool.total_liquidity, 500_000); // Not inflated to 520,000
    assert_eq!(pool.interest_reserve, 20_000); // Interest tracked separately
}

#[test]
fn test_repay_partial_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, _, _, token) = setup(&env);
    let provider = Address::generate(&env);
    let borrower = Address::generate(&env);
    
    mint(&env, &token, &provider, 1_000_000);
    mint(&env, &token, &borrower, 1_000_000);
    
    c.deposit(&provider, &500_000i128);
    c.borrow(&borrower, &1u64, &100_000i128, &86_400u64);
    
    // Repay less than principal (partial repayment)
    // Note: Current implementation removes borrow record after any repayment
    c.repay(&borrower, &1u64, &50_000i128);
    let pool = c.get_pool_state();
    
    // Should reduce borrowed by 50k
    assert_eq!(pool.total_borrowed, 50_000);
    assert_eq!(pool.total_liquidity, 500_000); // Unchanged
    assert_eq!(pool.interest_reserve, 0); // No interest in partial payment
    
    // Borrow record should be removed
    assert!(c.get_borrow(&1u64).is_none());
}
