## Leader Operations

### LEADER_CLAIM

This Operation claims the leader's block reward anonymously.

#### Payload

```python
class ClaimRequest:
    rewards_root: zkhash # Merkle root used in the proof for voucher membership
    voucher_nf: zkhash
    public_key: ZkPublicKey
```

#### Proof

  The provider proves that they have won a proof of Leadership before the start of the current epoch, i.e., their reward voucher is indeed in the voucher set: [Proof of Claim](#proof-of-claim).

#### Execution gas

  Leader Claim Operations have a fixed Execution Gas cost of `EXECUTION_LEADER_CLAIM_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
mantle_txhash: zkhash
claim : ClaimRequest
last_voucher_root: zkhash # The last root of the voucher Merkle tree
                          # at the start of the epoch
voucher_nullifier_set: set[zkhash]
proof: ProofOfClaim
```

  *Validate*

```python
assert claim.voucher_nf not in voucher_nullifier_set
assert claim.rewards_root == last_voucher_root
validate_proof(claim, proof, mantle_txhash)
```

#### Execution

  *Given*

```python
claim: ClaimRequest

ledger: Ledger
voucher_nullifier_set: set[zkhash]
leaders_rewards: TokenValue   # The pool of tokens to be claim by leaders
leader_reward: TokenValue     # The amount one leader can claim
```

  *Execution*

  1. Add `claim.voucher_nf` to the `voucher_nullifier_set`.
  2. Denoting by `leader_reward` the amount defined for leader rewards in [Leaders Reward](bedrock-anonymous-leaders-reward.md#leaders-reward), construct a single output note with value leader_reward under the public key defined in the payload, and insert it into the Ledger:
      ```python
      output_note=Note(
          value = leader_reward
          public_key = claim.public_key,
      )
      claim_id = derive_op_id(claim)
      ledger.execute_adding(claim_id, [output_note])
      ```

  3. Reduce the leader’s reward `leaders_rewards` value by the same amount (without ZK proof).

#### Example

```python
secret_voucher = 0xDEADBEAF;
reward_voucher = leader_claim_voucher(secret_voucher)
voucher_nullifier = leader_claim_nullifier(secret_voucher)

claim=ClaimRequest(
    rewards_root=REWARDS_MERKLE_TREE.root(),
    voucher_nf=voucher_nullifier,
    public_key=leader_one_time_key
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[<fee_note>], outputs=[<change_note>])

tx = MantleTx(
    ops=[Op(opcode=LEADER_CLAIM, payload=encode(claim)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

claim_proof = claim.prove(
    secret_voucher,
    REWARDS_MERKLE_TREE.path(leaf=reward_voucher),
    mantle_txhash(tx)
)

SignedMantleTx(
    tx=tx,
    op_proofs=[claim_proof, transfer.prove(fee_note_sk)]
)
```

