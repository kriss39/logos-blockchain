## Proof of Work Operations

Validators must maintain the following state to process proof of work Operations:

```python
pow_reward_pool: TokenValue      # Reserve the rewards are paid from
epoch_pow_reward: TokenValue     # Reward per claim, fixed for the epoch
difficulty_reward: PowTarget     # the reward threshold, retargeted every block
pow_nullifiers: set[zkhash]      # Spent solutions, retained for the acceptance window
block_slots: dict[hash, SlotNumber]  # Slots of recently seen blocks, for the window check
```

`PowTarget`, the acceptance window, and the maintenance of `pow_reward_pool`, `epoch_pow_reward` and `difficulty_reward` between blocks are specified in [Proof of Work](proof-of-work.md).

### CLAIM_POW_REWARD

This Operation claims a reward from the proof of work [reward pool](proof-of-work.md#reward-pool) by presenting a puzzle solution.

#### Payload

```python
class ClaimPowRewardOp:
    epoch_nonce: zkhash        # Epoch nonce the solution was found against
    block_hash: hash           # Recent canonical block the solution is anchored to
    public_key: ZkPublicKey    # Key the reward note is paid to
```

#### Proof

  A [ZkSignature](#zero-knowledge-signature-scheme-zksignature) by the secret key corresponding to `public_key`, over the transaction's `mantle_txhash`. The signature proves knowledge of that secret key, so a solution cannot be found by searching over public keys directly.

#### Execution gas

  Claim Operations have a fixed Execution Gas cost of `EXECUTION_CLAIM_POW_REWARD_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
mantle_txhash: zkhash
claim: ClaimPowRewardOp            # the CLAIM_POW_REWARD payload
claim_proof: ZkSignature           # the op_proofs entry for this Operation

current_slot: SlotNumber           # slot of the block including this claim
epoch_nonce_current: zkhash        # Cryptarchia epoch nonce of the current epoch
epoch_nonce_previous: zkhash       # and of the epoch before it
WINDOW: SlotNumber                 # the acceptance window, in slots
difficulty_reward: PowTarget       # retargeted every block
pow_nullifiers: set[zkhash]        # spent solutions, retained for WINDOW
pow_reward_pool: TokenValue
epoch_pow_reward: TokenValue
```

  The epoch nonces are the Cryptarchia epoch nonce $`\eta`$ of [Epoch Nonce](cryptarchia-v1-protocol.md#epoch-nonce), and `WINDOW` is derived in [Acceptance Window](proof-of-work.md#acceptance-window).

  *Validate*

```python
# 1. Claiming must be enabled for this block: the pool must be able to cover a reward.
assert epoch_pow_reward > 0
assert pow_reward_pool >= epoch_pow_reward

# 2. The referenced block must be canonical and within the acceptance window.
block = get_block_from_hash(claim.block_hash)   # None if unknown or not canonical
assert block is not None
assert 0 <= current_slot - block.slot <= WINDOW

# 3. The solution must have been found against the current or the previous epoch.
assert claim.epoch_nonce in (epoch_nonce_current, epoch_nonce_previous)

# 4. The ticket must satisfy the reward threshold.
puzzle_ticket = zkhash(claim.public_key,
                       FiniteField(claim.block_hash, byte_order="little", modulus=p),
                       claim.epoch_nonce)
assert puzzle_ticket < difficulty_reward

# 5. The solution must not have been claimed before. The nullifier is the ticket.
assert puzzle_ticket not in pow_nullifiers

# 6. The claim must be signed by the key the reward is paid to.
assert ZkSignature_verify(mantle_txhash, claim_proof, [claim.public_key])
```

#### Execution

  *Given*

```python
claim: ClaimPowRewardOp
puzzle_ticket: zkhash              # computed in validation step 4

ledger: Ledger
pow_reward_pool: TokenValue
epoch_pow_reward: TokenValue       # fixed for the epoch
pow_nullifiers: set[zkhash]
```

  *Execution*

  1. Add `puzzle_ticket` to the `pow_nullifiers` set. The entry is retained until the claim's referenced block leaves the [acceptance window](proof-of-work.md#acceptance-window).
  2. Construct a single output note of value `epoch_pow_reward` under the public key given in the payload, and insert it into the Ledger:
      ```python
      output_note = Note(
          value = epoch_pow_reward,
          public_key = claim.public_key,
      )
      claim_id = derive_op_id(claim)
      ledger.execute_adding(claim_id, [output_note])
      ```

  3. Reduce the `pow_reward_pool` by the same amount:
      ```python
      pow_reward_pool = checked_uint64(pow_reward_pool - epoch_pow_reward)
      ```

#### Example

```python
claim = ClaimPowRewardOp(
    epoch_nonce=get_current_epoch_nonce(),
    block_hash=recent_canonical_block_hash(),
    public_key=reward_pk,          # a key whose ticket satisfies difficulty_reward
)

# The reward note is spendable by the following Operation, so it pays the fee
reward_note_id = derive_note_id(derive_op_id(claim), 0,
                                Note(value=epoch_pow_reward, public_key=reward_pk))
transfer = Transfer(inputs=[reward_note_id], outputs=[<change_note>])

tx = MantleTx(
    ops=[Op(opcode=CLAIM_POW_REWARD, payload=encode(claim)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

SignedMantleTx(
    tx=tx,
    op_proofs=[ZkSignature(reward_sk, mantle_txhash(tx)), transfer.prove(reward_sk)]
)
```

