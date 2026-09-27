// Deposit USDC into the Heliobond InvestmentVault on testnet using only the
// SDK (#624 acceptance criterion).
//
//   SECRET_KEY=S... USDC_AMOUNT=1000000000 npm run example:deposit
//
// USDC must already be approved for the vault (see docs/INTEGRATION.md).
import { Keypair } from "@stellar/stellar-sdk";
import { basicNodeSigner } from "@stellar/stellar-sdk/contract";
import { InvestmentVault, requireContractIds } from "../src/index.js";

const secret = process.env.SECRET_KEY;
if (!secret) throw new Error("set SECRET_KEY to a funded testnet secret key");
const amount = BigInt(process.env.USDC_AMOUNT ?? "1000000000"); // 100 USDC (7 dp)

const net = requireContractIds("testnet");
const keypair = Keypair.fromSecret(secret);
const { signTransaction, signAuthEntry } = basicNodeSigner(keypair, net.networkPassphrase);

const vault = new InvestmentVault.Client({
  contractId: net.investmentVault,
  networkPassphrase: net.networkPassphrase,
  rpcUrl: net.rpcUrl,
  publicKey: keypair.publicKey(),
  signTransaction,
  signAuthEntry,
});

const tx = await vault.deposit({ from: keypair.publicKey(), usdc_amount: amount });
const { result } = await tx.signAndSend();
console.log(`Deposited ${amount} stroops of USDC; shares minted: ${result}`);
