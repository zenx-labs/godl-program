# Solana Program Deployment Runbook

This is an interactive deployment runbook designed to be executed by Claude. Each step should be run sequentially, with Claude providing output and guidance.

## Prerequisites

- `solana-verify` CLI installed
- `solana` CLI installed and configured
- Environment variable `HELIUS_API_KEY` set with your Helius API key

## Configuration

```bash
PROGRAM_ID="mineWsRs2Rmw2jPMkVbgAbDjV1E23yQ8TEodaX3iza4"
MULTISIG_AUTHORITY="3sPEBsgaoPFNFMUEGxqMFtvvvAYhCSbFDNiW6jSNqfYR"
GITHUB_REPO="https://github.com/zenx-labs/godl-program"
LIBRARY_NAME="godl"
```

| Variable             | Description                                                |
| -------------------- | ---------------------------------------------------------- |
| `PROGRAM_ID`         | The on-chain program address                               |
| `MULTISIG_AUTHORITY` | The Squads multisig address that controls program upgrades |
| `GITHUB_REPO`        | The GitHub repository URL for the program source           |
| `LIBRARY_NAME`       | The library name (used for binary name and temp files)     |

---

## Deployment Steps

### Step 0: Review changes to be deployed

Fetch the currently deployed commit from Solana Verify and summarize all changes.

**Fetch deployed commit:**

```bash
DEPLOYED_COMMIT=$(curl -s "https://verify.osec.io/status/${PROGRAM_ID}" \
  -H "Accept: application/json" | grep -o '"commit":"[^"]*"' | cut -d'"' -f4)
echo "Currently deployed commit: $DEPLOYED_COMMIT"
```

**Show commits since deployed version:**

```bash
git log --oneline $DEPLOYED_COMMIT..HEAD
```

**Show files changed:**

```bash
git diff --stat $DEPLOYED_COMMIT..HEAD
```

**Show code changes:**

```bash
git diff $DEPLOYED_COMMIT..HEAD
```

**Claude**: Present a clear summary of:

1. Number of commits being deployed
2. List of commits with their messages
3. Files changed and their purpose
4. Any breaking changes or risk assessment

Then use `AskUserQuestion` to confirm the user wants to proceed with deployment.

---

### Step 1: Build the program

Build the program with solana-verify to ensure reproducible builds. First remove any existing `.so` artifacts so you can be sure the deployed binary is freshly built.

```bash
rm -f target/deploy/*.so
solana-verify build --library-name "$LIBRARY_NAME"
```

**Expected output**: Build completes successfully, `target/deploy/${LIBRARY_NAME}.so` is created.

---

### Step 2: Generate temporary buffer keypair

Create a new keypair for the program buffer. This keypair's address will be the buffer address.

```bash
BUFFER_KEYPAIR="/tmp/${LIBRARY_NAME}-buffer-$(date +%s).json"
solana-keygen new -o "$BUFFER_KEYPAIR" --no-bip39-passphrase --force
```

**Save the keypair path and get the buffer address**:

```bash
BUFFER_ADDRESS=$(solana address -k "$BUFFER_KEYPAIR")
echo "Buffer keypair: $BUFFER_KEYPAIR"
echo "Buffer address: $BUFFER_ADDRESS"
```

---

### Step 3: Write program to buffer

Deploy the program binary to the buffer account on mainnet.

```bash
solana program write-buffer "target/deploy/${LIBRARY_NAME}.so" \
  --buffer "$BUFFER_KEYPAIR" \
  --with-compute-unit-price 1000000 \
  --url "https://mainnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}"
```

**Expected output**: Buffer write completes, confirms the buffer address.

---

### Step 4: Verify buffer integrity

Verify the on-chain buffer matches the local build by comparing executable hashes.

```bash
NETWORK_URL="https://mainnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}"

# Hash of your locally built program
solana-verify get-executable-hash "target/deploy/${LIBRARY_NAME}.so"

# Hash of the on-chain buffer account
solana-verify get-buffer-hash -u "$NETWORK_URL" "$BUFFER_ADDRESS"
```

**Expected output**: The two hashes match exactly, confirming buffer integrity. If they differ, do not proceed — re-run the build (Step 1) and re-upload the buffer (Step 3).

---

### Step 5: Set buffer authority to multisig

Transfer the buffer authority to the multisig so the vault can execute the upgrade.

```bash
solana program set-buffer-authority "$BUFFER_ADDRESS" \
  --new-buffer-authority "$MULTISIG_AUTHORITY" \
  -um
```

**Expected output**: Buffer authority updated successfully.

---

### Step 6: Create the multisig transactions in Squads

**Manual step required**: This step prints everything you need for both multisig operations at once — the program upgrade and the Solana Verify PDA transaction — so you can create them in a single Squads session.

Print both outputs together:

```bash
COMMIT_HASH=$(git rev-parse HEAD)
REMOTE_URL=$(git remote get-url origin | sed 's/\.git$//' | sed 's|git@github.com:|https://github.com/|')
BUFFER_REFUND="botHfLbBG8oSrohhfCF63xj3LhpBjJrYQkyE27gA4rN"
UPGRADE_NAME="Upgrade ${LIBRARY_NAME} @ ${COMMIT_HASH:0:7}"

PDA_TX=$(solana-verify export-pda-tx \
  "$GITHUB_REPO" \
  --library-name "$LIBRARY_NAME" \
  --program-id "$PROGRAM_ID" \
  --uploader "$MULTISIG_AUTHORITY" \
  --encoding base58 \
  --compute-unit-price 0)

cat <<EOF

============== 1) SQUADS — NEW PROGRAM UPGRADE ==============

  Name           : ${UPGRADE_NAME}
  Buffer Address : ${BUFFER_ADDRESS}
  Buffer Refund  : ${BUFFER_REFUND}
  Commit Link    : ${REMOTE_URL}/commit/${COMMIT_HASH}

============== 2) SOLANA-VERIFY PDA TX (base58) =============
copy everything between the lines below

------------------------------------------------------------
${PDA_TX}
------------------------------------------------------------
EOF
```

Then, in Squads (one session, two transactions):

1. Open Squads and navigate to your multisig
2. Start a new program upgrade and paste in the four values from section **1**
3. Create the Solana Verify PDA transaction using the base58 string from section **2**
4. Once both transactions are created in Squads, return here and confirm

**Claude**: Print the block above so each field / the base58 string is easy to copy, then use `AskUserQuestion` to confirm when both transactions have been created in Squads.

---

### Step 7: Wait for multisig approval

**Manual step required**: All multisig signers must approve both transactions in Squads.

1. Notify all multisig signers that the deployment is ready for approval
2. Each signer must go to Squads and sign the program upgrade and PDA verification transactions
3. Once all required signatures are collected, the transactions will execute automatically

**Claude**: Use `AskUserQuestion` to prompt the user to confirm when the multisig transactions have been fully executed.

---

### Step 8: Cleanup temporary keypair

Once the deployment is fully complete, clean up the temporary keypair.

```bash
rm -f "$BUFFER_KEYPAIR"
echo "Cleaned up temporary buffer keypair"
```

---

### Step 9: Submit verification job

Submit a verification job to confirm the on-chain program matches the GitHub source.

**Note**: This command may fail due to rate limits or temporary hash mismatches. Retry until successful.

```bash
solana-verify remote submit-job \
  --program-id "$PROGRAM_ID" \
  --uploader "$MULTISIG_AUTHORITY"
```

**Claude**: If this command fails, wait a few seconds and retry. Continue retrying until the job is successfully submitted.

**Expected output**: Verification job submitted successfully.

---

## Notes for Claude

When executing this runbook:

**General:**

- Run each step sequentially using the Bash tool
- If any step fails unexpectedly, stop and report the error before proceeding
- Verify `HELIUS_API_KEY` environment variable is set before Step 3
- Read the Configuration section variables and use them in all commands

**Step-specific instructions:**

- **Step 0**: Fetch the deployed commit from the Solana Verify API using `$PROGRAM_ID`, present a clear summary of all changes, and use `AskUserQuestion` to confirm before proceeding
- **Step 2**: Store `BUFFER_KEYPAIR` and `BUFFER_ADDRESS` for use in subsequent steps
- **Step 4**: Compare the executable hash and the buffer hash; they must be identical. If they differ, stop and report the mismatch before proceeding
- **Step 6**: Print both multisig outputs together — the Squads upgrade modal values (name, buffer address, buffer refund, commit link) and the base58 PDA verification transaction — so all multisig operations can be done in one Squads session. Both are created manually in Squads — do not automate them. Use `AskUserQuestion` to confirm both transactions have been created
- **Step 7**: Use `AskUserQuestion` to prompt the user to confirm the multisig transactions have been fully executed
- **Step 8**: Only run cleanup after user confirms deployment is complete
- **Step 9**: Retry the verification job command until it succeeds (may fail due to rate limits or temporary issues)
