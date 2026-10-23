## Channel Operations

Channels allow Zones to post their updates on chain. Channels form virtual chains that overlay on top of the Cryptarchia blockchain. Clients and Followers of a Zone can watch its channel to learn the state of that Zone. Each channel has an associated balance, enabling bridging between Zones and Bedrock.

### Message Ordering

Channels form virtual chains by having each message reference its parent message. The order of messages in these channels is enforced by the sequencer by building a hash chain of messages, i.e. new messages reference the previous messages through a parent hash. Given that Cryptarchia has long finality times, these message parent references allow Zone sequencers to continue to post new updates to channels without having to wait for finality. No matter how Cryptarchia forks and reorgs, the channel messages from honest sequencers will eventually be re-included in a way that satisfies the virtual chain order.

Configurations form a second hash chain within the channel: each configuration names the configuration it supersedes, so a pending reconfiguration stays valid while the sequencer keeps posting inscriptions.

The first time a message is sent to an unclaimed channel, the key that signs the initial message becomes the only accredited key in the list (Note that this key may correspond to a threshold signature key). Accredited keys of a channel forms a committee that can configure the channel, withdraw funds and take turns to write messages to that channel following a round-robin algorithm. Configuring a channel includes modifying the list of accredited keys, the round-robin parameters and the required number of signatures to withdraw funds or establish a new configuration.

Validators must maintain the following state to process channel Operations:

```python
channels: dict[ChannelId, ChannelState] # ChannelId is 32 bytes

class ChannelState:
    # Channel Configuration
    accredited_keys: list[Ed25519PublicKey]  # limited to 65 535 keys
    configuration_threshold: u16  # indicating how many keys are
                                  # required to update the configuration

    # Message Ordering
    tip_hash: hash         # Last message of the channel
    config_tip_hash: hash  # Last configuration of the channel

    # Decentralized Sequencing
    tip_slot: Slot
    tip_sequencer: u16      # indicating the actual
                            # sequencer position in the list of accredited keys
    tip_sequencer_starting_slot: Slot
    posting_timeframe: u32  # number of slots (0 = infinity)
    posting_timeout: u32    # number of slots (0 = no timeout)

    # Bridging
    transfer_threshold: u16  # indicating how many keys are
                             # required to transfer or withdraw funds from the channel

def default_channel(block_slot: Slot, keys: list[Ed25519PublicKey]) -> ChannelState:
    return ChannelState(
        tip_hash = ZERO,
        config_tip_hash = ZERO,
        tip_slot = block_slot,
        accredited_keys = keys,
        tip_sequencer = 0,
        tip_sequencer_starting_slot = block_slot,
        posting_timeframe = 0,
        posting_timeout = 0,
        configuration_threshold = 1,
        transfer_threshold = 1)
```

Note that the user chooses the ChannelId mapping to the ChannelState (but it’s restricted to 32 bytes). We don't currently impose restrictions on it, but we may do so in the future to prevent undesirable behaviors.

### Decentralized Sequencing

To determine which sequencer is currently authorized to send messages, we use a round-robin algorithm. When a message is posted to a channel, the following algorithm is used to determine who the sequencer is:

```python
# Round Robin algorithm determining the new sequencer index and the
# new sequencer starting slot
def round_robin(block_slot: Slot, channel: ChannelState) -> (u16, u64):
    elapsed_slots = block_slot - channel.tip_slot
    if elapsed_slots >= channel.posting_timeout and channel.posting_timeout != 0:
        # Get the number of sequencers that get timed out
        sequencers_timed_out = elapsed_slots // channel.posting_timeout
        index = (
            (channel.tip_sequencer + sequencers_timed_out)
            % len(channel.accredited_keys)
        )
        starting_slot = (
            channel.tip_slot
            + sequencers_timed_out * channel.posting_timeout
        )
    else:
        # Get the number of timeframes elapsed to get who is the sequencer
        tip_sequencer_duration = block_slot - channel.tip_sequencer_starting_slot
        index = (
            (channel.tip_sequencer + (tip_sequencer_duration // channel.posting_timeframe))
            % len(channel.accredited_keys)
        )
        starting_slot = (
            channel.tip_sequencer_starting_slot
            + (tip_sequencer_duration // channel.posting_timeframe) * channel.posting_timeframe
        )
    return (index, starting_slot)
```

### Bridging

Channels let their bridged funds keep participating in Proof of Stake. When a user deposits funds into a channel, the deposited notes stay on the ledger and are not turned into inert collateral. They are consumed and immediately re-created as channel notes that continue to count toward Proof of Stake and can still be used to create PoLs (see [Channel Notes](#channel-notes)). Two goals motivate this design:

- **More PoS participation, stronger security.** Funds deposited into a channel would otherwise leave the staking set. Keeping them as channel notes means the capital backing the application layer also backs consensus security, so bridging does not shrink the stake that secures the chain.
- **No split between security and application.** A user no longer has to choose between staking funds or using them in a channel. The same funds do both at once. They stay usable inside the channel while still earning Proof of Leadership rewards, so capital is never fragmented between the two.

**Ownership vs. staking power.** A `CHANNEL_DEPOSIT` separates the two rights that a normal note bundles together:

- *Ownership* moves to the channel. The note is registered in the ledger's `channel_notes` set with the channel as its owner, and the channel keeps full control over it. The deposited notes are consumed and re-created identically: they keep their value and `ZkPublicKey`, receive a new `NoteId` derived from the deposit's `OpId`, and are registered as channel-owned. The channel is now the party responsible for the note.
- *Staking power* stays with the `ZkPublicKey` carried by the note. That key does not confer ownership. It only delegates the note's value for PoL creation. Whoever controls the key is the one allowed to turn the note into a PoL and collect the resulting rewards. On deposit this key is still the depositor's, so the user keeps the PoS participation power they had before bridging.

Because the channel owns the note but does not hold the delegated key, the note earns rewards for the key holder, never for the channel itself.

**Ageing.** Because the deposit re-creates the notes under a new `NoteId`, a deposited note restarts the ageing process and must age again before it can create a PoL. Bridged funds still count toward Proof of Stake, so the goals above hold, but the participation is not continuous across the deposit.

**What each party can do.**

| Party | Can | Cannot |
|---|---|---|
| Holder of the note's `ZkPublicKey` (by default, the depositor) | Use the note to create a PoL and earn its leader rewards | Spend the note, withdraw it, reassign it, or use it as service stake |
| Channel sequencers (owner of the note) | Reassign the note to a different `ZkPublicKey` (`CHANNEL_TRANSFER`) and spend it to fund withdrawals (`CHANNEL_WITHDRAW`), both without `ZkSignature` verification | Use the note as service stake, or earn PoL rewards without first assigning the note to their own key |

This makes delegated staking explicit. Sequencers can assign a channel note to their own `ZkPublicKey` and earn the Proof of Leadership rewards it produces, but those rewards always follow the assigned key, so the channel earns nothing merely by owning the note. Conversely, ownership never leaving the channel is exactly what lets sequencers redelegate value or cover withdrawals at any time without a user signature.

**Warning: a deposit is a transfer of custody.** Depositors must understand that channel note handling is fully defined by the channel. Once a `CHANNEL_DEPOSIT` is executed the note belongs to the channel, and its sequencers can reassign it to any `ZkPublicKey` with `CHANNEL_TRANSFER` or release it to whoever they choose with `CHANNEL_WITHDRAW`, at any time and without any signature from the depositor. The ledger enforces no return path to the original depositor. Holding the note's `ZkPublicKey` grants PoS participation power only and never a claim on the value, so it confers no ability to recover the funds. A user who deposits into a dishonest or faulty channel has no on-chain recourse. Deposit only into channels you trust to honour their own withdrawal policy.

### CHANNEL_INSCRIBE

Write a message to a channel with the message data being permanently stored on the Logos Blockchain.

#### Payload

```python
class Inscribe:
    channel: ChannelId       # 32 bytes Channel being written to
    inscription : bytes      # Message to be written on the blockchain
    parent: hash             # Previous message in the channel
    signer: Ed25519PublicKey # Identity of message sender
```

#### Proof

```python
Ed25519Signature
```

#### Execution Gas

  Channel Inscribe Operations have a fixed Execution Gas cost of `EXECUTION_CHANNEL_INSCRIBE_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
txhash: hash
msg: Inscribe
sig: Ed25519Signature

channels: dict[ChannelId, ChannelState]
block_slot: Slot
```
 
  *Validate*

```python
if msg.channel in channels:
    chan = channels[msg.channel]
    current_sequencer_index = round_robin(block_slot, chan)[0]

    # Ensure the signer is the one authorized to write to the channel
    assert msg.signer == chan.accredited_keys[current_sequencer_index]

    # Ensure message is continuing the channel sequence
    assert msg.parent == chan.tip_hash
else:
    # Channel will be created automatically upon execution
    # Ensure that this message is the genesis message (parent == ZERO)
    assert msg.parent == ZERO

# Ensure the msg signer signature
assert Ed25519_verify(txhash, msg.signer, sig)
```

#### Execution

  *Given*

```python
msg: Inscribe
sig: Ed25519Signature

channels: dict[ChannelId, ChannelState]
block_slot: Slot
```

  *Execute*

  1. If the channel does not exist, create it just-in-time.
      ```python
      if msg.channel not in channels:
          channels[msg.channel] = default_channel(block_slot, [msg.signer])
      ```

  2. Update the channel sequencer.
      ```python
      chan = channels[msg.channel]
      (new_sequencer_index, new_sequencer_starting_slot) = round_robin(block_slot, chan)

      chan.tip_sequencer_starting_slot = new_sequencer_starting_slot
      chan.tip_sequencer = new_sequencer_index
      ```

  3. Update the channel tip.
      ```python
      chan = channels[msg.channel]
      chan.tip_hash = hash(encode(msg))
      chan.tip_slot = block_slot
      ```

#### Example

```python
# Build the inscription
greeting = Inscription(
    channel=CHANNEL_EARTH,
    inscription=b"Live long and prosper",
    parent=ZERO
    signer=spock_pk
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[<spocks_note_id>], outputs=[<change_note>])

# Wrap it in a transaction
tx = MantleTx(
    ops=[Op(opcode=CHANNEL_INSCRIBE, payload=encode(greeting)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

# Sign the transaction
signed_tx = SignedMantleTx(
    tx=tx,
    op_proofs=[Ed25519_sign(mantle_txhash(tx), spock_sk),
               transfer.prove(spock_sk)]
)

# Send the transaction to the mempool
mempool.push(signed_tx)
```

### CHANNEL_CONFIG

Overwrite the configuration of a channel.

#### Payload

```python
class ChannelConfig:
    channel: ChannelId
    parent: hash             # Previous configuration of the channel
    keys: list[Ed25519PublicKey]
    posting_timeframe: u32
    posting_timeout: u32
    configuration_threshold: u16
    transfer_threshold: u16
```

#### Proof

A Channel Config is authorized by a threshold of the channel's accredited keys using [Multiple Ed25519 Signatures Verification](#multiple-ed25519-signatures-verification).

```python
class ChannelConfigOpProof:
    signatures: list[Ed25519Signature] # signatures from configuration_threshold
    indexes: list[u16]  # signatures of accredited keys with their index.
                        # indexes must be ordered from smallest to
                        # biggest without duplication
```

#### Execution Gas

  Channel Config Operations have a linear Execution Gas cost equal to `EXECUTION_CHANNEL_CONFIG_GAS * configuration_threshold`, where `configuration_threshold` is the one held in the channel state, and `0` for a channel that does not exist yet. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
txhash: zkhash
config: ChannelConfig
proof: ChannelConfigOpProof
channels: dict[ChannelId, ChannelState]
```

  *Validate*

```python
assert config.configuration_threshold > 0
assert config.transfer_threshold > 0
assert len(config.keys) > 0
assert len(config.keys) < 2^16
# The configuration threshold must be reachable with the accredited keys,
# otherwise the channel would be locked out of any future reconfiguration
assert config.configuration_threshold <= len(config.keys)

if config.channel in channels:
    chan = channels[config.channel]

    # Ensure the configuration is extending the last configuration
    # of the channel
    assert config.parent == chan.config_tip_hash

    # Verify the configuration_threshold signatures (see Appendix)
    MultiEd25519_verify(txhash,
                        proof.signatures,
                        proof.indexes,
                        chan.accredited_keys,
                        chan.configuration_threshold)
else:
    # Channel will be created automatically upon execution
    # Ensure that this configuration is the genesis configuration
    assert config.parent == ZERO

    # No key is accredited yet, so the threshold to verify against is 0
    # and the proof must carry no signature and no index (see Appendix)
    MultiEd25519_verify(txhash,
                        proof.signatures,
                        proof.indexes,
                        [],
                        0)
```

#### Execution

  *Given*

```python
config: ChannelConfig

channels: dict[ChannelId, ChannelState]
block_slot: Slot
```

  *Execute*

  1. If the channel does not exist, create it just-in-time.

      ```python
      if config.channel not in channels:
          channels[config.channel] = default_channel(block_slot, config.keys)
      ```

  2. Update the configuration.

      ```python
      chan = channels[config.channel]

      # Update Channel Configuration Parameters
      chan.accredited_keys = config.keys
      chan.configuration_threshold = config.configuration_threshold

      # Update Decentralized Sequencing Parameters
      chan.tip_slot = block_slot
      chan.tip_sequencer = 0
      chan.tip_sequencer_starting_slot = block_slot
      chan.posting_timeframe = config.posting_timeframe
      chan.posting_timeout = config.posting_timeout

      # Update Bridging Parameters
      chan.transfer_threshold = config.transfer_threshold
      ```

  3. Update the configuration tip.

      ```python
      chan = channels[config.channel]
      chan.config_tip_hash = hash(encode(config))
      ```

#### Example

  Suppose the unique sequencer of Zone A wants to add a key to the list of accredited keys:

```python
# Given a key to add and the current configuration tip of the channel
new_sequencer_pk: Ed25519PublicKey
zone_a_config_tip: hash

# The unique sequencer encodes the update and builds the payload
config = ChannelConfig(
    channel=ZONE_A,
    parent=zone_a_config_tip,
    keys=[old_sequencer_pk, new_sequencer_pk],
    posting_timeframe = 5000,
    posting_timeout = 500,
    configuration_threshold = 2,
    transfer_threshold = 1
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[old_sequencer_funds], outputs=[<change_note>])

tx = MantleTx(
    ops=[Op(opcode=CHANNEL_CONFIG, payload=encode(config)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

signed_tx = SignedMantleTx(
    tx=tx,
    op_proofs=[[[Ed25519_sign(mantle_txhash(tx), old_sequencer_sk)], [0]],
               transfer.prove(old_sequencer_sk)]
)
```

### CHANNEL_DEPOSIT

Deposit notes to a channel. The inputs are consumed and re-created as channel notes under a new `NoteId`, which resets their ageing and prevents the deposit from being replayed after a withdrawal.

#### Payload

```python
class ChannelDeposit:
    channel: ChannelId
    inputs: list[NoteId]  # the notes to be consumed and re-created as channel notes
    metadata: bytes
```

#### Proof

  A Channel Deposit proves the ownership of the notes being consumed using a [Zero Knowledge Signature Scheme (ZkSignature)](#zero-knowledge-signature-scheme-zksignature).

```python
ZkSignature
```

#### Execution Gas

  Channel Deposit Operations have a fixed Execution Gas cost of `EXECUTION_CHANNEL_DEPOSIT_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
mantle_txhash: zkhash # zkhash of mantle tx containing this ledger tx
deposit: ChannelDeposit
deposit_proof: ZkSignature

channels: dict[ChannelId, ChannelState]

ledger: Ledger
```

  *Validate*

  1. Verify that the channel exist
      ```python
      assert deposit.channel in channels
      ```

  2. Ensure all inputs are spendable and not already channel notes.
      ```python
      ledger.assert_spendable(deposit.inputs)
      ```

  3. Validate ownership over deposited notes.
      ```python
      input_notes = [ledger[input_note_id] for input_note_id in deposit.inputs]
      input_pks = [note.public_key for note in input_notes]
      assert ZkSignature_verify(mantle_txhash, deposit_proof, input_pks)
      ```

#### Execution

  *Given*

```python
deposit: ChannelDeposit

channels: dict[ChannelId, ChannelState]

ledger: Ledger
```

  *Execute*

Consume the inputs and create the same Note with new NoteId as channel notes owned by the channel.

```python
# read the notes that are being moved into the channel
notes_to_add = [ledger[input_note_id] for input_note_id in deposit.inputs]

# consume the inputs, which are regular notes and not registered in channel_notes
ledger.execute_spending(deposit.inputs)

# re-create them as channel notes under a new NoteId
deposit_id = derive_op_id(deposit)
ledger.execute_adding(deposit_id, notes_to_add, deposit.channel)
```

#### Example

  Suppose Alice wants to make a deposit of 50 tokens on Zone A.

```python
# Alice encodes her deposit
deposit = ChannelDeposit(
    channel=ZONE_A,
    inputs=[alice_deposit_note_id]    # This is a note of 50 tokens
    metadata=b"deposit to address: 0x..."
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[Alice_funds], outputs=[<change_note>])


tx = MantleTx(
    ops=[Op(opcode=CHANNEL_DEPOSIT, payload=encode(deposit)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

signed_tx = SignedMantleTx(
    tx=tx,
    op_proofs=[deposit.prove(Alice_sk), transfer.prove(Alice_sk)],
)
```

A Zone that credits a deposit in its own state must be sure the deposit really lands on-chain. If the Zone reflects the deposit through a `CHANNEL_INSCRIBE` posted in a separate Mantle Transaction, a reorganization can reorder the two so that the inscription is included while the deposit is not, leaving the Zone crediting funds it never received. Two options avoid this:

- Wait for the deposit to be finalized before interpreting it, at the cost of the finalization delay.
- Make the inscription conditional on the deposit, by including a `CHANNEL_TRANSFER` that consumes the deposited note in the same Mantle Transaction as the inscription. Mantle Transactions execute atomically, so the inscription is included only if the deposited note exists and is consumed. This removes the waiting period entirely.

The second option resets the ageing of the value. A `CHANNEL_TRANSFER` consumes its inputs and creates new notes, so the resulting note starts the ageing process again and must age before it can create a PoL. A `CHANNEL_DEPOSIT` resets ageing for the same reason, since it consumes its inputs and re-creates them under a new `NoteId`. A `CHANNEL_WITHDRAW` keeps the `NoteId` of the notes it releases and therefore never resets ageing.

### CHANNEL_WITHDRAW

Withdraw notes from a channel.

#### Payload

```python
class ChannelWithdraw:
    channel: ChannelId
    inputs: list[NoteId]
```

#### Proof

A Channel Withdraw is authorized by a threshold of the channel's accredited keys using [Multiple Ed25519 Signatures Verification](#multiple-ed25519-signatures-verification).

```python
class ChannelWithdrawOpProof:
    signatures: list[Ed25519Signature] # exactly transfer_threshold signatures
    indexes: list[int]    # signatures of accredited keys with their index
                          # indexes must be ordered from smallest to
                          # biggest without duplication
```

#### Execution Gas

  Channel Withdraw Operations have a linear Execution Gas cost equal to `EXECUTION_CHANNEL_WITHDRAW_GAS * transfer_threshold`, where `transfer_threshold` is the one held in the channel state. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
txhash: zkhash
withdrawal: ChannelWithdraw
proof: ChannelWithdrawOpProof

channels: dict[ChannelId, ChannelState]
ledger: Ledger
```

  *Validate*

  1. Check that the channel exists
      ```python
      assert withdrawal.channel in channels
      ```

  2. Check that the inputs are valid and belongs to the channel
      ```python
      ledger.assert_spendable(withdrawal.inputs, withdrawal.channel)
      ```

  3. Check the signatures (see [Multiple Ed25519 Signatures Verification](#multiple-ed25519-signatures-verification))
      ```python
      MultiEd25519_verify(txhash,
                          proof.signatures,
                          proof.indexes,
                          channels[withdrawal.channel].accredited_keys,
                          channels[withdrawal.channel].transfer_treshold)
      ```

#### Execution

  *Given*

```python
withdrawal: ChannelWithdraw

channels: dict[ChannelId, ChannelState]
ledger: Ledger
```

  *Execute*

Remove the inputs from channel notes owned by the channel. The notes are neither consumed nor re-created: they keep their NoteId, value and ZkPublicKey, and are simply unregistered in the channel_notes set.
```python
for note_id in withdrawal.inputs:
    ledger.channel_notes.pop(note_id)
```
#### Example

  Suppose the unique sequencer of Zone A wants to withdraw 50 tokens.

```python
# Sequencer encodes his withdrawal
withdrawal = ChannelWithdraw(
    channel=ZONE_A,
    inputs  = [Channel_note_id]
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[Sequencer_funds], outputs=[<change_note>])

tx = MantleTx(
    ops=[Op(opcode=CHANNEL_WITHDRAW, payload=encode(withdrawal)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

signed_tx = SignedMantleTx(
    tx=tx,
    op_proofs=[[[Ed25519_sign(mantle_txhash(tx), sequencer_sk)],[0]],
               transfer.prove(Sequencer_node_sk)],
)
```

### CHANNEL_TRANSFER

Assign funds from a channel to new `ZkPublicKey`. This funds are only usable to participate in PoS and to withdraw from the channel.

#### Payload

```python
class ChannelTransfer:
    channel: ChannelId
    inputs: list[NoteId]
    outputs: list[Note]
```

#### Proof

```python
class ChannelTransferOpProof:
    signatures: list[Ed25519Signature] # signature from transfer_threshold keys
    indexes: list[int]    # signatures of accredited keys with their index.
                          # indexes must be ordered from smallest to biggest without duplication
```

#### Execution Gas

`CHANNEL_TRANSFER` Operations have a linear Execution Gas cost equal to `EXECUTION_CHANNEL_TRANSFER_GAS * transfer_threshold`, where `transfer_threshold` is the one held in the channel state. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

*Given*

```python
txhash: zkhash
chan_transfer: ChannelTransfer
proof: ChannelTransferOpProof

channels: dict[ChannelId, ChannelState]
ledger: Ledger
```

*Validate*

1. Check that the outputs are valid

```python
ledger.assert_valid_output(chan_transfer.outputs)
```

2. Check that the channel exists

```python
assert chan_transfer.channel in channels
```

3. Check that the inputs are valid and belongs to the channel

```python
ledger.assert_spendable(chan_transfer.inputs, chan_transfer.channel)
```

4. Check the balance

```python
input_amount = checked_uint64(sum(ledger.get_note(input).value for input in chan_transfer.inputs))
output_amount = checked_uint64(sum(output.value for output in chan_transfer.outputs))
assert input_amount == output_amount
```

5. Check the signatures (see [Multiple Ed25519 Signatures Verification](#multiple-ed25519-signatures-verification))
```python
MultiEd25519_verify(txhash,
					proof.signatures,
                    proof.indexes,
                    channels[chan_transfer.channel].accredited_keys,
                    channels[chan_transfer.channel].transfer_treshold)
```

#### Execution

*Given*

```python
chan_transfer: ChannelTransfer

channels: dict[ChannelId, ChannelState]
ledger: Ledger
```

*Execute*

1. Remove inputs from the ledger

```python
ledger.execute_spending(chan_transfer.inputs, chan_transfer.channel)
```

2. Add outputs to the ledger.

```python
chan_transfer_id = derive_op_id(chan_transfer)
ledger.execute_adding(chan_transfer_id, chan_transfer.outputs, chan_transfer.channel)
```

#### Example

Suppose the unique sequencer of Zone A wants to attribute 50 tokens to themself.

```python
# Sequencer encodes their assignation
chan_transfer = ChannelTransfer(
    channel=ZONE_A,
    inputs = [Channel_note_id]
    outputs = [Note(pk=alice, value=50)]
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[Sequencer_funds], outputs=[<change_note>])

tx = MantleTx(
    ops=[Op(opcode=CHANNEL_TRANSFER, payload=encode(chan_transfer)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

signed_tx = SignedMantleTx(
    tx=tx,
    op_proofs=[[[Ed25519_sign(mantle_txhash(tx), sequencer_sk)],[0]],
                              transfer.prove(Sequencer_node_sk)],
)
```

