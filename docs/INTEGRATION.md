# Integration Guide

**Issue:** [#98](https://github.com/Heliobond/contracts/issues/98)

This guide covers integrating with the Heliobond smart contracts from a JavaScript/TypeScript frontend or Node.js backend using the [Stellar SDK](https://stellar.github.io/js-stellar-sdk/).

---

## Prerequisites

```bash
npm install @stellar/stellar-sdk
```

You also need:
- A funded Stellar account (testnet faucet: [friendbot](https://friendbot.stellar.org))
- The deployed contract IDs (see deployment summary in CI or ask the team)

---

## Contract addresses

| Contract | Testnet ID | Description |
|----------|-----------|-------------|
| `ProjectRegistry` | `CXXX…` | Manages projects, whitelist, governance |
| `InvestmentVault` | `CYYY…` | Manages deposits, shares, yield, insurance |
| USDC SAC | `CZZZ…` | USDC Stellar Asset Contract on testnet |

> Replace `CX…`, `CY…`, `CZ…` with the actual IDs from the latest deployment.

---

## Connecting to the network

```typescript
import {
  SorobanRpc,
  TransactionBuilder,
  Networks,
  Keypair,
  Contract,
  nativeToScVal,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";

const RPC_URL = "https://soroban-testnet.stellar.org";
const server   = new SorobanRpc.Server(RPC_URL);
const network  = Networks.TESTNET;

const keypair  = Keypair.fromSecret("SXXX…your secret key…");
const account  = await server.getAccount(keypair.publicKey());
```

---

## Helper: build, simulate, sign, submit

```typescript
async function invokeContract(
  contractId: string,
  method: string,
  args: xdr.ScVal[],
  keypair: Keypair,
): Promise<xdr.ScVal> {
  const contract = new Contract(contractId);
  const account  = await server.getAccount(keypair.publicKey());

  const tx = new TransactionBuilder(account, {
    fee: "100000",
    networkPassphrase: network,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  // Simulate to get resource fees and footprint
  const sim = await server.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = SorobanRpc.assembleTransaction(tx, sim).build();
  prepared.sign(keypair);

  const result = await server.sendTransaction(prepared);
  if (result.status === "ERROR") {
    throw new Error(`Submit failed: ${JSON.stringify(result.errorResult)}`);
  }

  // Poll for confirmation
  let response = await server.getTransaction(result.hash);
  while (response.status === "NOT_FOUND") {
    await new Promise(r => setTimeout(r, 1000));
    response = await server.getTransaction(result.hash);
  }
  if (response.status !== "SUCCESS") {
    throw new Error(`Transaction failed: ${response.status}`);
  }
  return response.returnValue!;
}
```

---

## ProjectRegistry — key operations

### Check if an address is whitelisted

```typescript
const REGISTRY = "CXXX…";

const result = await server.simulateTransaction(
  new TransactionBuilder(account, { fee: "100", networkPassphrase: network })
    .addOperation(new Contract(REGISTRY).call(
      "get_whitelist",
      nativeToScVal(keypair.publicKey(), { type: "address" }),
    ))
    .setTimeout(30)
    .build()
);
const isWhitelisted: boolean = scValToNative((result as any).result.retval);
```

### Create a project

```typescript
const projectId = scValToNative(await invokeContract(
  REGISTRY,
  "create_project",
  [
    nativeToScVal(keypair.publicKey(), { type: "address" }),  // creator
    nativeToScVal("ipfs://QmYourHash",  { type: "string" }),  // uri
    nativeToScVal(0n,                   { type: "u64" }),     // maturity_date (0 = open-ended)
  ],
  keypair,
));
console.log("Project created with ID:", projectId);
```

### Create a governance proposal

```typescript
const proposalId = scValToNative(await invokeContract(
  REGISTRY,
  "create_proposal",
  [
    nativeToScVal(keypair.publicKey(), { type: "address" }),
    nativeToScVal("Increase insurance premium to 1%", { type: "string" }),
    nativeToScVal(BigInt(7 * 24 * 3600), { type: "u64" }), // 7-day voting period
  ],
  keypair,
));
```

### Cast a vote

Query the investor's HBS share balance first, then pass it as the weight:

```typescript
const VAULT = "CYYY…";

// Read HBS balance (view — no auth, no fee beyond simulation)
const balanceSim = await server.simulateTransaction(
  new TransactionBuilder(account, { fee: "100", networkPassphrase: network })
    .addOperation(new Contract(VAULT).call(
      "balance",
      nativeToScVal(keypair.publicKey(), { type: "address" }),
    ))
    .setTimeout(30)
    .build()
);
const hbsBalance: bigint = scValToNative((balanceSim as any).result.retval);

await invokeContract(
  REGISTRY,
  "cast_vote",
  [
    nativeToScVal(keypair.publicKey(), { type: "address" }),
    nativeToScVal(proposalId,  { type: "u32" }),
    nativeToScVal(true,        { type: "bool" }),  // support = true
    nativeToScVal(hbsBalance,  { type: "i128" }),
  ],
  keypair,
);
```

---

## InvestmentVault — key operations

### Approve USDC and deposit

Soroban SAC tokens use SEP-41. Approve the vault to spend your USDC first:

```typescript
const USDC_SAC = "CZZZ…";
const DEPOSIT_AMOUNT = 1_000_000_000n; // 100 USDC (7 decimals)

// Step 1: approve
await invokeContract(
  USDC_SAC,
  "approve",
  [
    nativeToScVal(keypair.publicKey(), { type: "address" }), // from
    nativeToScVal(VAULT,              { type: "address" }), // spender
    nativeToScVal(DEPOSIT_AMOUNT,     { type: "i128" }),
    nativeToScVal(99999999n,          { type: "u32" }),     // expiration_ledger
  ],
  keypair,
);

// Step 2: deposit
const sharesMinted = scValToNative(await invokeContract(
  VAULT,
  "deposit",
  [
    nativeToScVal(keypair.publicKey(), { type: "address" }),
    nativeToScVal(DEPOSIT_AMOUNT,      { type: "i128" }),
  ],
  keypair,
));
console.log("Shares minted:", sharesMinted.toString());
```

### Read portfolio (view — free)

```typescript
const portfolioSim = await server.simulateTransaction(
  new TransactionBuilder(account, { fee: "100", networkPassphrase: network })
    .addOperation(new Contract(VAULT).call(
      "get_portfolio",
      nativeToScVal(keypair.publicKey(), { type: "address" }),
    ))
    .setTimeout(30)
    .build()
);
const portfolio = scValToNative((portfolioSim as any).result.retval);
console.log("Portfolio:", portfolio);
// {
//   shares: bigint,
//   usdc_value: bigint,
//   claimable_yield: bigint,
//   share_of_pool_bps: bigint,
//   total_deposited: bigint,
// }
```

### Claim yield

```typescript
const claimed = scValToNative(await invokeContract(
  VAULT,
  "claim_yield",
  [nativeToScVal(keypair.publicKey(), { type: "address" })],
  keypair,
));
console.log("Yield claimed (USDC stroops):", claimed.toString());
```

### Withdraw shares

```typescript
const usdcReturned = scValToNative(await invokeContract(
  VAULT,
  "withdraw",
  [
    nativeToScVal(keypair.publicKey(), { type: "address" }),
    nativeToScVal(sharesMinted / 2n,  { type: "i128" }),  // redeem half
  ],
  keypair,
));
```

---

## Error handling

All contract panics surface as Soroban `HostError` codes in the simulation or transaction result. Common patterns:

| Panic message | Cause | Resolution |
|---------------|-------|------------|
| `"not whitelisted"` | Creator not in whitelist | Ask admin to call `set_whitelist` |
| `"deposit must be positive"` | Amount ≤ 0 | Validate input before submitting |
| `"deposit exceeds maximum"` | Amount > 1 billion USDC | Split into multiple deposits |
| `"uri too short"` | URI < 8 bytes | Provide a valid IPFS or HTTPS URI |
| `"insufficient deployable USDC"` | Vault liquid balance minus insurance reserve < amount | Wait for more deposits or reduce amount |
| `"already voted"` | Voter already cast a vote on this proposal | UI should check `has_voted` before showing vote button |
| `"voting period too short"` | Duration < 86 400 s | Use at least 1 day |
| `"insurance already claimed"` | Payout already made for this project | Check `InsuranceClaimed` state first |

```typescript
try {
  await invokeContract(VAULT, "deposit", [...], keypair);
} catch (err: any) {
  if (err.message.includes("deposit exceeds maximum")) {
    console.error("Deposit too large — split into multiple transactions");
  } else {
    throw err;
  }
}
```

---

## Events

Listen to contract events using the Stellar Horizon API or an indexer:

```typescript
// Via Horizon (events endpoint)
const resp = await fetch(
  `https://horizon-testnet.stellar.org/contracts/${VAULT}/events?limit=20`
);
const { _embedded: { records } } = await resp.json();
records.forEach((ev: any) => {
  console.log(ev.type, ev.value);
});
```

Key event topics by contract:

| Contract | Topic | Fired when |
|----------|-------|-----------|
| `InvestmentVault` | `deposit` | Investor deposits USDC |
| `InvestmentVault` | `withdraw` | Investor withdraws |
| `InvestmentVault` | `yield_received` | Owner posts yield |
| `InvestmentVault` | `yield_claimed` | Investor claims yield |
| `InvestmentVault` | `insurance_claimed` | Default payout made |
| `ProjectRegistry` | `project_created` | New project registered |
| `ProjectRegistry` | `project_updated` | Impact scores updated |
| `ProjectRegistry` | `project_certified` | Certification status changed |
| `ProjectRegistry` | `proposal_created` | Governance proposal opened |
| `ProjectRegistry` | `vote_cast` | Vote recorded |
| `ProjectRegistry` | `proposal_executed` | Proposal finalised |
