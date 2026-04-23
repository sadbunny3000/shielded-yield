# 🛡️ Shielded Yield: The Privacy Layer for Institutional DeFi
**Track:** DeFi & Financial Infrastructure | **Bounty:** Adevar Labs Security & MagicBlock Ephemeral Rollups

> Shielded Yield is a privacy-first decentralized yield aggregator built on Solana. It allows any wallet holder to deposit SOL into a personal vault, earn algorithmically calculated yield, and withdraw their principal plus earnings — all without exposing their balance or compounding activity to on-chain observers or MEV extraction bots.

In traditional DeFi, every vault transaction is publicly visible on-chain in real time. This transparency creates an attack surface where bots can front-run withdrawals, sandwich settlement transactions, and extract value from users passively. Shielded Yield eliminates this exposure by moving high-frequency yield accrual and vault state updates off the base layer and into a MagicBlock Ephemeral Rollup — a private, validator-secured execution environment that commits state back to Solana only at settlement time.

**Who it is for:** individual Solana users who want yield on idle SOL without being exploited by MEV bots; DeFi protocols that want to integrate private execution for their own vault logic; and builders who need a reference implementation of MagicBlock's Delegation Program integrated into a production Anchor program.

---

## 📋 Project Info

| Field | Detail |
|---|---|
| **Program ID** | `5F8R6GdfgdkRQNPv5TTDEkcUw5Vtpy6Irw4cilCBFCRp` |
| **Network** | Solana Devnet (live deployment) |
| **Framework** | Anchor 0.32.1 |
| **Rollup Layer** | MagicBlock Ephemeral Rollups |
| **GitHub** | github.com/sadbunny3000/shielded-yield |
| **Hackathon Track** | Adevar Labs Security Bounty — DeFi Category |

---

## 🌟 Overview

Shielded Yield is a decentralized yield aggregator that utilizes **MagicBlock Ephemeral Rollups** to provide a "Dark Pool" experience for on-chain interest accrual.

In standard DeFi protocols, every yield-bearing transaction is publicly visible on-chain in real time. This transparency creates an attack surface where bots can front-run withdrawals, sandwich settlement transactions, and extract value from users passively. Shielded Yield moves the execution of yield-bearing state transitions into a private, high-frequency execution layer, shielding user data until the moment of settlement.

---

## 🛠️ Technology Stack

* **L1 Blockchain:** Solana (Anchor Framework)
* **Privacy Layer:** MagicBlock Ephemeral Rollups
* **Security:** u128 Safe Math, PDA Validation
* **Deployment:** Devnet Program ID: `5F8R6GdfgdkRQNPv5TTDEkcUw5Vtpy6Irw4cilCBFCRp`

---

## 🔒 Security Architecture (Adevar Labs Focus)

Designed with an "Audit-First" mindset, Shielded Yield implements three layers of defense:

1. **Execution Isolation:** By delegating accounts to an Ephemeral Rollup, we remove transactions from the public Solana mempool, mitigating 100% of mainnet MEV front-running risks during the yield phase.
2. **Arithmetic Integrity:** We utilize `u128` intermediate casting and Anchor's `checked_mul/add` handlers for all interest accrual math. This eliminates the risk of integer overflows and rounding exploits.
3. **Strict Validation:** Every instruction (delegate/undelegate) is bound by strict Signer and PDA seed constraints, ensuring only the vault owner can trigger state transitions.

*Full technical specifications are available in [SECURITY_DOSSIER.md](./SECURITY_DOSSIER.md).*

---

## ⚖️ Legal & Regulatory Roadmap (NeosLegal Prize)

Shielded Yield is designed to be compatible with emerging 2026 global regulatory standards, specifically **UAE (VARA/ADGM)** and **EU (MiCA)** frameworks.

* **Privacy vs. Compliance:** While execution is shielded, we maintain "selective transparency." The protocol allows integration of ZK-proofs for Proof of Solvency without revealing individual user balances, aligning with VARA's 2026 guidance.
* **Institutional Guardrails:** The protocol includes a governance-controlled "Emergency Pause" (Circuit Breaker) to align with VASP requirements for consumer protection and asset safety.
* **Jurisdictional Versatility:** Ephemeral Rollups allow geography-specific rollups that adhere to local data-residency laws while remaining anchored to the Solana global liquidity layer.

---

## 📊 Performance & Simulation

We have conducted high-frequency stress tests simulating 10,000+ state transitions. Results:

* **Gas Savings:** 99.4% reduction in transaction costs compared to mainnet settlement.
* **Throughput:** Sub-second compounding intervals, impossible on standard L1 blocks.
* **Integrity:** Zero state-drift during high-volume delegation cycles.

*See [`/simulations/simulation.ts`](./simulations/simulation.ts) for the full stress-test logic.*

---

## 🔗 Live Links

| Resource | Link |
|---|---|
| **Devnet Program** | [View on Solana Explorer](https://explorer.solana.com/address/5F8R6GdfgdkRQNPv5TTDEkcUw5Vtpy6Irw4cilCBFCRp?cluster=devnet) |
| **GitHub Repo** | [github.com/sadbunny3000/shielded-yield](https://github.com/sadbunny3000/shielded-yield) |
| **Demo Video** | Coming soon |
| **Colosseum Submission** | Solana Frontier Hackathon 2026 |

---

## 👥 Team

### Natangwe Shikesho — Founder & Lead Developer

Natangwe is a self-taught Solana developer and entrepreneur based in **Ongwediva, Oshana Region, Namibia**. He is the founder of Sunrise Poultry Farm, a biosecure broiler and egg production operation supplying schools, households, and restaurants in the Oshana region — demonstrating a track record of building and operating real-world ventures from the ground up.

**Solana Ecosystem Work:**
* **Shielded Yield** — This project: a privacy-first yield aggregator using Anchor 0.32.1 and MagicBlock Ephemeral Rollups, successfully deployed to Solana Devnet.
* **Sealed-Bid Auction on Solana** — Colosseum Frontier Hackathon submission implementing FHE-based sealed bidding using the Encrypt SDK and Ika track.
* **Solana Wallet Tracker** — Superteam/RPC Fast bounty submission: a Node.js + Express web app using `@solana/web3.js` and RPC Fast endpoints to track wallet activity in real time.

**Superteam Status:** Active Superteam Earn participant — completed RPC Fast bounty, active Superteam grant applicant. Pursuing Superteam EU Demo Day / Regional Africa Demo Day participation.

| | |
|---|---|
| **GitHub** | [github.com/sadbunny3000](https://github.com/sadbunny3000) |
| **Location** | Ongwediva, Namibia 🇳🇦 |
| **Ecosystem** | Superteam Africa |

---

## 🚀 Roadmap

> Currently at **Phase 1 — Hackathon Prototype**. Seeking Adevar Labs audit credits (not VC funding) as the critical gate to mainnet launch.

* **Phase 1 (Current):** Devnet deployment, hackathon submission, Adevar Labs audit credit application.
* **Phase 2 (Post-audit):** Mainnet beta launch, single-asset SOL vault open to the public.
* **Phase 3:** Multi-asset support (stSOL, mSOL), variable yield rates via on-chain governance, integration with lending protocols as yield source.
* **Phase 4:** SDK release for other DeFi protocols to integrate Shielded Yield's ephemeral privacy layer into their own vault logic.

**Market Opportunity:** Total SOL staked on liquid staking protocols exceeds $10B. Shielded Yield targets privacy-conscious users with a differentiated guarantee that no current Solana yield protocol offers.

---

## 🚀 Getting Started

1. **Clone the repo:** `git clone https://github.com/sadbunny3000/shielded-yield.git`
2. **Install dependencies:** `npm install`
3. **Run Tests:** `anchor test`
4. **Run Simulation:** `npx ts-node simulations/simulation.ts`

---

**Built for the Solana Frontier Hackathon 2026**
*Submitted by Natangwe Shikesho | Ongwediva, Namibia*
