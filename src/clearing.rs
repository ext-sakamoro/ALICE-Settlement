/*
    ALICE-Settlement
    Copyright (C) 2026 Moroya Sakamoto
*/

use std::collections::HashMap;

use crate::netting::NetObligation;

/// Account balance for clearing.
#[derive(Debug, Clone)]
pub struct ClearingAccount {
    pub account_id: u64,
    /// Available balance in ticks (cash equivalent).
    pub balance: i64,
    /// Margin held.
    pub margin_held: i64,
}

/// Error returned when clearing an obligation fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearingError {
    /// The specified account was not found in the clearing house.
    AccountNotFound(u64),
    /// The account has insufficient balance to meet the obligation.
    InsufficientBalance {
        account_id: u64,
        required: i64,
        available: i64,
    },
}

/// Per-obligation clearing outcome.
#[derive(Debug, Clone)]
pub struct ClearingResult {
    pub obligation: NetObligation,
    pub success: bool,
    pub error: Option<ClearingError>,
}

/// Central clearing house.
///
/// Maintains account balances and processes net obligations from the netting
/// engine. On success, debits the deliverer and credits the receiver.
pub struct ClearingHouse {
    accounts: HashMap<u64, ClearingAccount>,
}

impl ClearingHouse {
    /// Create an empty clearing house.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    /// Register an account with an initial balance.
    ///
    /// If the account already exists, the balance is replaced.
    #[inline(always)]
    pub fn register_account(&mut self, id: u64, initial_balance: i64) {
        self.accounts.insert(
            id,
            ClearingAccount {
                account_id: id,
                balance: initial_balance,
                margin_held: 0,
            },
        );
    }

    /// Look up an account by identifier.
    #[inline(always)]
    pub fn get_account(&self, id: u64) -> Option<&ClearingAccount> {
        self.accounts.get(&id)
    }

    /// Attempt to clear a single net obligation.
    ///
    /// Checks that the deliverer has a balance of at least `net_payment`, then
    /// transfers `net_payment` from deliverer to receiver.
    pub fn clear_obligation(&mut self, obligation: &NetObligation) -> Result<(), ClearingError> {
        // Verify both accounts exist before mutating anything.
        if !self.accounts.contains_key(&obligation.deliverer_id) {
            return Err(ClearingError::AccountNotFound(obligation.deliverer_id));
        }
        if !self.accounts.contains_key(&obligation.receiver_id) {
            return Err(ClearingError::AccountNotFound(obligation.receiver_id));
        }

        // Balance check: deliverer existence was verified above.
        let deliverer_balance = if let Some(acc) = self.accounts.get(&obligation.deliverer_id) {
            acc.balance
        } else {
            return Err(ClearingError::AccountNotFound(obligation.deliverer_id));
        };

        if deliverer_balance < obligation.net_payment {
            return Err(ClearingError::InsufficientBalance {
                account_id: obligation.deliverer_id,
                required: obligation.net_payment,
                available: deliverer_balance,
            });
        }

        // Perform the transfer; both accounts were verified above.
        if let Some(acc) = self.accounts.get_mut(&obligation.deliverer_id) {
            acc.balance -= obligation.net_payment;
        }

        if let Some(acc) = self.accounts.get_mut(&obligation.receiver_id) {
            acc.balance += obligation.net_payment;
        }

        Ok(())
    }

    /// Attempt to clear all obligations, returning per-obligation results.
    ///
    /// Obligations that fail do not roll back previously cleared obligations.
    pub fn clear_all(&mut self, obligations: &[NetObligation]) -> Vec<ClearingResult> {
        obligations
            .iter()
            .map(|ob| match self.clear_obligation(ob) {
                Ok(()) => ClearingResult {
                    obligation: ob.clone(),
                    success: true,
                    error: None,
                },
                Err(e) => ClearingResult {
                    obligation: ob.clone(),
                    success: false,
                    error: Some(e),
                },
            })
            .collect()
    }
}

impl Default for ClearingHouse {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_obligation(
        symbol_hash: u64,
        deliverer_id: u64,
        receiver_id: u64,
        net_quantity: u64,
        net_payment: i64,
    ) -> NetObligation {
        NetObligation {
            symbol_hash,
            deliverer_id,
            receiver_id,
            net_quantity,
            net_payment,
            trade_count: 1,
        }
    }

    #[test]
    fn test_register_account() {
        let mut ch = ClearingHouse::new();
        ch.register_account(1, 100_000);

        let acc = ch.get_account(1).unwrap();
        assert_eq!(acc.account_id, 1);
        assert_eq!(acc.balance, 100_000);
        assert_eq!(acc.margin_held, 0);

        assert!(ch.get_account(99).is_none());
    }

    #[test]
    fn test_clear_success() {
        let mut ch = ClearingHouse::new();
        ch.register_account(100, 50_000); // deliverer
        ch.register_account(200, 10_000); // receiver

        let ob = make_obligation(0xABCD, 100, 200, 10, 5_000);
        let result = ch.clear_obligation(&ob);
        assert!(result.is_ok());

        assert_eq!(ch.get_account(100).unwrap().balance, 45_000);
        assert_eq!(ch.get_account(200).unwrap().balance, 15_000);
    }

    #[test]
    fn test_clear_insufficient_balance() {
        let mut ch = ClearingHouse::new();
        ch.register_account(100, 1_000); // not enough
        ch.register_account(200, 0);

        let ob = make_obligation(0xABCD, 100, 200, 10, 5_000);
        let result = ch.clear_obligation(&ob);

        assert!(result.is_err());
        match result.unwrap_err() {
            ClearingError::InsufficientBalance {
                account_id,
                required,
                available,
            } => {
                assert_eq!(account_id, 100);
                assert_eq!(required, 5_000);
                assert_eq!(available, 1_000);
            }
            other => panic!("unexpected error: {:?}", other),
        }

        // Balances must be unchanged after failure
        assert_eq!(ch.get_account(100).unwrap().balance, 1_000);
        assert_eq!(ch.get_account(200).unwrap().balance, 0);
    }

    #[test]
    fn test_clear_unknown_account() {
        let mut ch = ClearingHouse::new();
        ch.register_account(100, 50_000);
        // Receiver (200) not registered

        let ob = make_obligation(0xABCD, 100, 200, 10, 5_000);
        let result = ch.clear_obligation(&ob);

        assert!(result.is_err());
        match result.unwrap_err() {
            ClearingError::AccountNotFound(id) => {
                assert_eq!(id, 200);
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_clear_all_partial_failures() {
        let mut ch = ClearingHouse::new();
        ch.register_account(100, 50_000);
        ch.register_account(200, 500); // too low for second obligation
        ch.register_account(300, 20_000);

        let ob1 = make_obligation(0x0001, 100, 300, 5, 2_000); // succeeds
        let ob2 = make_obligation(0x0002, 200, 300, 3, 5_000); // fails (balance 500 < 5000)
        let ob3 = make_obligation(0x0003, 100, 200, 2, 1_000); // succeeds

        let results = ch.clear_all(&[ob1, ob2, ob3]);
        assert_eq!(results.len(), 3);

        assert!(results[0].success);
        assert!(!results[1].success);
        assert!(results[1].error.is_some());
        assert!(results[2].success);

        // Verify final balances
        assert_eq!(ch.get_account(100).unwrap().balance, 47_000); // 50000 - 2000 - 1000
        assert_eq!(ch.get_account(200).unwrap().balance, 1_500); // 500 + 1000 (received from ob3)
        assert_eq!(ch.get_account(300).unwrap().balance, 22_000); // 20000 + 2000 (ob1)
    }

    #[test]
    fn test_register_account_replaces_balance() {
        let mut ch = ClearingHouse::new();
        ch.register_account(1, 1_000);
        ch.register_account(1, 9_999); // overwrite
        let acc = ch.get_account(1).unwrap();
        assert_eq!(acc.balance, 9_999);
        assert_eq!(acc.margin_held, 0);
    }

    #[test]
    fn test_clear_zero_payment() {
        // A zero-payment obligation should succeed and leave balances unchanged.
        let mut ch = ClearingHouse::new();
        ch.register_account(1, 500);
        ch.register_account(2, 500);
        let ob = make_obligation(0x01, 1, 2, 0, 0);
        assert!(ch.clear_obligation(&ob).is_ok());
        assert_eq!(ch.get_account(1).unwrap().balance, 500);
        assert_eq!(ch.get_account(2).unwrap().balance, 500);
    }

    #[test]
    fn test_clear_deliverer_unknown() {
        // Only deliverer is missing — error must reference deliverer.
        let mut ch = ClearingHouse::new();
        ch.register_account(200, 10_000);
        let ob = make_obligation(0xAA, 999, 200, 1, 100);
        match ch.clear_obligation(&ob) {
            Err(ClearingError::AccountNotFound(id)) => assert_eq!(id, 999),
            other => panic!("expected AccountNotFound(999), got {:?}", other),
        }
    }

    #[test]
    fn test_clear_receiver_unknown() {
        // Deliverer exists, receiver is missing.
        let mut ch = ClearingHouse::new();
        ch.register_account(100, 10_000);
        let ob = make_obligation(0xBB, 100, 888, 1, 100);
        match ch.clear_obligation(&ob) {
            Err(ClearingError::AccountNotFound(id)) => assert_eq!(id, 888),
            other => panic!("expected AccountNotFound(888), got {:?}", other),
        }
    }

    #[test]
    fn test_clear_all_empty_obligations() {
        let mut ch = ClearingHouse::new();
        let results = ch.clear_all(&[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_clearing_error_eq() {
        let e1 = ClearingError::AccountNotFound(42);
        let e2 = ClearingError::AccountNotFound(42);
        assert_eq!(e1, e2);

        let e3 = ClearingError::InsufficientBalance {
            account_id: 1,
            required: 100,
            available: 50,
        };
        let e4 = ClearingError::InsufficientBalance {
            account_id: 1,
            required: 100,
            available: 50,
        };
        assert_eq!(e3, e4);
        assert_ne!(e1, e3);
    }

    #[test]
    fn test_default_clearing_house() {
        let ch = ClearingHouse::default();
        assert!(ch.get_account(0).is_none());
    }

    #[test]
    fn test_sequential_clear_same_pair() {
        // Two consecutive obligations between the same pair — balances accumulate.
        let mut ch = ClearingHouse::new();
        ch.register_account(1, 100_000);
        ch.register_account(2, 0);
        let ob1 = make_obligation(0x01, 1, 2, 1, 10_000);
        let ob2 = make_obligation(0x02, 1, 2, 1, 20_000);
        assert!(ch.clear_obligation(&ob1).is_ok());
        assert!(ch.clear_obligation(&ob2).is_ok());
        assert_eq!(ch.get_account(1).unwrap().balance, 70_000);
        assert_eq!(ch.get_account(2).unwrap().balance, 30_000);
    }

    #[test]
    fn test_exact_balance_obligation_succeeds() {
        // Clearing exactly the available balance should succeed.
        let mut ch = ClearingHouse::new();
        ch.register_account(1, 5_000);
        ch.register_account(2, 0);
        let ob = make_obligation(0xCC, 1, 2, 1, 5_000);
        assert!(ch.clear_obligation(&ob).is_ok());
        assert_eq!(ch.get_account(1).unwrap().balance, 0);
        assert_eq!(ch.get_account(2).unwrap().balance, 5_000);
    }
}
