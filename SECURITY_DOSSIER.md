# 🛡️ Shielded Yield: Security Architecture Dossier
**Target Bounty:** Adevar Labs Security Track ($50,000)
**Protocol Layer:** MagicBlock Ephemeral Rollups

## 1. State Isolation (The "Shield")
Standard DeFi vaults on Solana are vulnerable to mempool monitoring. Shielded Yield mitigates this by delegating the Vault account to an **Ephemeral Rollup**.
* **Confidential Execution:** Yield calculations are performed within the rollup's isolated execution environment.
* **Anti-MEV:** Because the state transitions occur off the mainnet mempool, bots cannot front-run the interest distribution.

## 2. Mathematical Integrity
To prevent "Economic Exploits" or "Drain Attacks," the following safeguards are implemented:
* **u128 Precision:** All interest accrual math utilizes 128-bit unsigned integers before final downcasting to prevent rounding errors.
* **Checked Arithmetic:** Every calculation uses Anchor's `checked_add` and `checked_mul` to ensure that an integer overflow cannot lead to an infinite mint.

## 3. Access Control & Validation
* **Signer Constraints:** Instructions like `delegate` and `undelegate` are hard-coded to require the signature of the `Vault.owner`.
* **PDA Verification:** The program strictly validates the seeds `[b"vault", owner, seed]` to prevent "Account Substitution" attacks.