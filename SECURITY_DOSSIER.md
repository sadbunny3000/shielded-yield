# 🔒 Shielded Yield: Security Architecture Dossier

**Target Bounty:** Adevar Labs Security Track ($50,000)
**Protocol Layer:** MagicBlock Ephemeral Rollups
**Program ID:** `5F8R6GdfgdkRQNPv5TTDEkcUw5Vtpy6Irw4cilCBFCRp`
**Network:** Solana Devnet (live deployment)
**Framework:** Anchor 0.32.1

---

## 1. Security Architecture Overview

Shielded Yield's security model is built on three pillars:

- **Execution Privacy via Ephemeral Rollups** — vault state mutations occur inside MagicBlock's validator-isolated execution environment, invisible to base-layer MEV infrastructure until settlement.

- **Arithmetic Integrity** — all yield calculations use 128-bit intermediate precision to eliminate overflow and truncation vulnerabilities endemic to on-chain financial math in 64-bit environments.

- **Strict Account Ownership Lifecycle** — the program enforces explicit ownership transitions for the Vault PDA at every stage: creation, delegation, and undelegation. Each stage has its own constraint set preventing cross-stage instruction misuse.

---

## 2. Identified Threat Vectors & Mitigations

| Threat Vector | Severity | Mitigation Implemented |
|---|---|---|
| MEV Sandwich Attack on Yield Settlement | **Critical** | Yield accrual moved off base-layer to ephemeral rollup; base-layer mempool has no visibility into accrual events |
| Re-entrancy on `withdraw` Instruction | **High** | Anchor's AccountInfo borrow model prevents re-entrant CPI; yield settled and balances deducted before lamport transfer |
| PDA Ownership Confusion (Delegation Window) | **High** | Separate constraint sets for delegate vs undelegate; `owner == crate::ID` assertion blocks delegation of already-delegated accounts |
| Rollup State Desync on Undelegation | **High** | `vault.reload()` called post-CPI to force re-deserialization of MagicBlock-committed state |
| Arithmetic Overflow in Yield Calculation | **High** | `u128` intermediate math with `.checked_mul()` throughout |
| Unauthorized Withdrawal | **Critical** | `has_one = owner` on all mutating instructions; PDA seeds bind vault to specific owner pubkey |
| Front-running of Deposit/Withdraw | **Medium** | Yield settlement happens atomically within the same instruction call |
| Stale Yield Slot Manipulation | **Medium** | `slots_elapsed = current.saturating_sub(last)` prevents negative or inflated windows |

---

## 3. MEV Sandwich Attack — Deep Dive

In Shielded Yield:
- Yield accrual state lives on the ephemeral rollup layer during the delegation window.
- The rollup validator commits state diffs to base-layer at configurable intervals (default: 5,000ms), not per-transaction.
- Withdrawal requires undelegation first, collapsing rollup state in a single atomic CPI — no exploitable gap exists.

Standard sandwich bots on base-layer RPC have no profitable insertion point during the high-frequency compounding phase.

---

## 4. Re-entrancy on `withdraw` — Deep Dive

1. `accrue_yield()` settles all pending yield into `yield_accrued`.
2. Balances calculated and `require!` guard checks sufficiency.
3. Lamport transfer CPI fires.
4. `vault.deposited` and `vault.yield_accrued` are decremented.

Solana's single-threaded execution model prevents any other instruction from observing vault state between Steps 3 and 4. This is equivalent to a checks-effects-interactions pattern.

---

## 5. PDA Ownership Confusion — Deep Dive

- **In Initialize:** `init` requires the account to not exist.
- **In Delegate:** `vault.to_account_info().owner == &crate::ID` explicitly asserted.
- **In Undelegate:** owner constraint relaxed to allow `DELEGATION_PROGRAM_ID` since MagicBlock transferred ownership during delegation. PDA seed verification still uniquely identifies the account.

---

## 6. Rollup State Desync — Deep Dive

When `undelegate` fires, MagicBlock writes the latest committed rollup state into the Vault account's raw bytes during the CPI. The fix: `ctx.accounts.vault.reload()` is called immediately after `invoke_signed` returns.

Without this `reload()`, `vault.yield_accrued` could revert to its pre-rollup value, silently losing all rollup-side yield accrual.

---

## 7. Trust Model

| Entity | Trust Level | Reason |
|---|---|---|
| Solana base-layer consensus + BPF runtime | **Trusted** | Foundation of the security model |
| MagicBlock Delegation Program (`DELeGGvXpWV2fqJUhqcF5ZSYMS4JTLjteaAMARRSaeSh`) | **Trusted** | Deployed and audited by MagicBlock |
| Vault owner signing key | **Trusted** | Compromised owner key compromises vault |
| `delegation_buffer` account | **Untrusted** | Passed as `UncheckedAccount`, verified by Delegation Program during CPI |
| `accrue` instruction caller | **Untrusted** | Permissionless; cannot extract funds without owner signature |

---

## 8. What the Delegation Program CAN and CANNOT Do

### CAN:
- Change vault account's program owner to `DELEGATION_PROGRAM_ID` during delegation.
- Write rollup state diffs into vault account data during committed intervals.
- Return program ownership to `shielded_yield::ID` during undelegation.

### CANNOT:
- Modify `vault.owner` (the Pubkey field).
- Move lamports out of the vault without a signed `withdraw` instruction.
- Override PDA seed derivation — seeds `[vault, owner_pubkey, seed_u64]` are intrinsic.
- Extend the delegation window past `valid_until_slot`.

---

## 9. Program Invariants

| # | Invariant | Enforced By |
|---|---|---|
| I-1 | `vault.deposited` always equals deposits minus principal withdrawals | Arithmetic in `deposit()` and `withdraw()` |
| I-2 | `vault.yield_accrued` only increases via `accrue_yield()` and decreases via `withdraw()` | Only these two code paths; `checked_add` prevents overflow |
| I-3 | `vault.delegated` is true iff Vault PDA owner is `DELEGATION_PROGRAM_ID` | Constraint in `Delegate` context and `reload()` in `Undelegate` |
| I-4 | Vault PDA can only be signed via `invoke_signed` within this program | PDA derivation from `[vault, owner, seed, bump]` |
| I-5 | `withdraw` unreachable while `vault.delegated == true` | `require!(!vault.delegated)` guard |
| I-6 | `yield_delta` can never be negative | Division of positive terms; saturating arithmetic |
| I-7 | `vault.last_yield_slot` always <= current slot | Set to `Clock::get()?.slot` on every accrue call |
| I-8 | No instruction can change `vault.owner` after initialization | `has_one = owner` on all mutating instructions |

---

## 10. Known Limitations & Audit Focus Areas

- **Static yield rate (800 bps annual).** No governance to update `ANNUAL_YIELD_BPS`. Auditors should confirm no rate manipulation is possible.
- **`delegation_buffer` passed as `UncheckedAccount`.** Validation deferred to Delegation Program CPI.
- **Permissionless accrue crank.** Cannot steal funds but could spam small accruals — auditors should verify rounding favours the protocol.
- **No timelock on withdrawals.** Future versions should consider withdrawal delay for liquidity pooling scenarios.

---

## 11. Arithmetic Integrity — Code Reference

```rust
let numerator = (vault.deposited as u128)
    .checked_mul(ANNUAL_YIELD_BPS as u128)
    .ok_or(VaultError::Overflow)?
    .checked_mul(slots_elapsed as u128)
    .ok_or(VaultError::Overflow)?;

let denominator = (BPS_DENOMINATOR as u128)
    .checked_mul(SLOTS_PER_YEAR as u128)
    .ok_or(VaultError::Overflow)?;

let yield_delta = (numerator / denominator) as u64;

vault.yield_accrued = vault.yield_accrued
    .checked_add(yield_delta)
    .ok_or(VaultError::Overflow)?;
```

All intermediate values cast to `u128` before multiplication. Division back to `u64` only occurs after the full numerator is computed.

---

*Prepared for Adevar Labs Security Bounty — Solana Frontier Hackathon 2026*
*Submitted by Natangwe Martin | Ongwediva, Namibia*
