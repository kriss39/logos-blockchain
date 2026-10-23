## Service Declaration Protocol (SDP) Operations

These Operations implement the [Service Declaration Protocol](bedrock-service-declaration-protocol.md).

Validators must keep the following state when implementing SDP Operations:

```python
service_notes: dict[NoteID, ServiceNote]
declarations: dict[DeclarationID, DeclarationInfo]

class ServiceNote:
    declarations: set[DeclarationID]
```

### Common SDP Structures

```python
class ServiceType(Enum):
    BN=0 # Blend Network; the one-byte discriminant is the canonical encoding

class Locator(bytes):
    # multiaddr binary form; the canonical encoding
    # (see SDP: Locators)
    def validate(self):
        assert len(self) <= 329
        assert validate_multiaddr(self)

class MinStake:
    stake_threshold: int # stake value
    epoch: EpochNumber # epoch number

class ServiceParameters:
    inactivity_period: NumberOfEpochs # number of epochs
    epoch: EpochNumber                # epoch number at which the Service Parameters were set

class DeclarationInfo:
    service: ServiceType
    locators: list[Locator]
    provider_id: Ed25519PublicKey
    zk_id: ZkPublicKey
    service_note_id: NoteId
    created: EpochNumber
    active: EpochNumber
    withdraw_at: EpochNumber | None
    # SDP ops updating a declaration must use monotonically increasing nonces
    nonce: int
```

### SDP_DECLARE

The service registration follows the definition given in [**Declaration Message**](bedrock-service-declaration-protocol.md#declaration-message):

#### Payload

```python
class DeclarationMessage:
    service_type: ServiceType
    locators: list[Locator]
    provider_id: Ed25519PublicKey
    zk_id: ZkPublicKey
    service_note_id: NoteId
```

Service notes are introduced in [Service notes](#service-notes) and serve as Service collaterals. They cannot be spent before the owner withdraw its participation from the declared service(s).

#### Proof

```python
class DeclarationProof:
    zk_sig: ZkSignature             # signature proving ownership over
                                    # service note and zk_id
    provider_sig: Ed25519Signature  # signature proving ownership of provider key
```

  see: [Zero Knowledge Signature Scheme (ZkSignature)](#zero-knowledge-signature-scheme-zksignature).

#### Execution Gas

  SDP Declare Operations have a fixed Execution Gas cost of `EXECUTION_SDP_DECLARE_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
txhash: zkhash                  # the txhash of the transaction we are validating
declaration: DeclarationMessage # the declaration we are validating
proof: DeclarationProof

min_stake: MinStake      # the (global) minimum stake setting
ledger: Ledger           # the set of unspent notes
service_notes: dict[NoteId, ServiceNote]
declarations: dict[NoteId, DeclarationInfo]
```

  *Validate*

  The declaration is verified according to [Declare](bedrock-service-declaration-protocol.md#declare).

  1. Ensure ownership over the service note, `zk_id` and `provider_id`.
      ```python
      assert ZkSignature_verify(
          txhash, proof.zk_sig, [note.public_key, declaration.zk_id]
      )
      assert Ed25519_verify(txhash, proof.provider_sig, provider_id)
      ```

  2. Ensure declaration does not already exist.
      ```python
      assert declaration_id(declaration) not in declarations
      ```

  3. Ensure the locators list is non-empty and has no more than 8 entries.
      ```python
      assert len(declaration.locators) >= 1
      assert len(declaration.locators) <= 8
      ```

  4. Ensure the service note exists and its value is sufficient for joining the service.
      ```python
      assert ledger.is_unspent(declaration.service_note_id)
      note = ledger.get_note(declaration.service_note_id)
      assert note.value >= min_stake.stake_threshold
      ```

  5. Ensure the note has not already been used for this service.
      ```python
      if declaration.service_note in service_notes:
          service_note = service_notes[declaration.service_note]
          services = [declarations[declare_id] for declare_id in service_note.declarations]
          assert declaration.service_type not in services
      ```

#### Execution

  *Given*

```python
declaration: DeclarationMessage # the declaration we are executing
current_epoch: EpochNumber
service_notes : dict[NoteId, ServiceNote]
```

  *Execute*

  1. Create the service note state if it doesn't already exist.
      ```python
      if declaration.service_note not in service_notes:
          service_notes[declaration.service_note_id] = ServiceNote(declarations=set())

      service_note = service_notes[declaration.service_note_id]
      ```

  2. Add this declaration to the service note.
      ```python
      declare_id = declaration_id(declaration)
      service_note.declarations.add(declare_id)
      ```

  3. Store the declaration as explained in [**Declaration Storage**](bedrock-service-declaration-protocol.md#declaration-storage).
      ```python
      declarations[declare_id] = DeclarationInfo(
          service: declaration.service
          locators: declaration.locators
          provider_id: declaration.provider_id
          zk_id: declaration.zk_id
          service_note_id: declaration.service_note_id
          declaration,
          created=current_epoch,
          active=current_epoch + 2,
          withdraw_at=None
          nonce=0
      )
      ```

#### Example

```python
# Assume `alice_note` is in the ledger:
alice_note = Utxo(
    txhash=0x2948904F2F0F479B8F8197694B30184B0D2ED1C1CD2A1EC0FB85D299A192A447,
    output_number=3,
    note=Note(value=500, public_key=alice_pk_1),
)

# Alice wishes to lock it to join the Blend network
declaration=DeclarationMessage(
    service_type=ServiceType.BN,
    locators=["/ip4/203.0.113.10/tcp/4001/p2p"],
    provider_id=alice_provider_pk,
    zk_id=alice_pk_2,
    service_note_id=alice_note.id()
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[fee_note_id], outputs=[])


tx = MantleTx(
    ops=[Op(opcode=SDP_DECLARE, payload=encode(declaration)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)
txhash = mantle_txhash(tx)

declaration_proof = DeclarationProof(
    # proof of ownership of the staked note and zk_id
    zk_sig=ZkSignature([alice_sk_1, alice_sk_2], txhash),
    # proof of ownership of the provider id
    provider_sig=Ed25519Signature(alice_provider_sk, txhash),
)

SignedMantleTx(
    tx=tx,
    op_proofs=[declaration_proof, transfer.prove(alice_sk_1)],
)
```

### SDP_WITHDRAW

The service withdrawal follows the definition given in [Withdraw Message](bedrock-service-declaration-protocol.md#withdraw-message).

#### Payload

```python
class WithdrawMessage:
    declaration: DeclarationID
    service_note_id: NoteId
    nonce: int
```

#### Proof

  A signature from the `zk_id` and the service note `pk` attached to the declaration is required for withdrawing from a service, (see [Zero Knowledge Signature Scheme (ZkSignature)](#zero-knowledge-signature-scheme-zksignature)).

```python
ZkSignature
```

#### Execution Gas

  SDP Withdraw Operations have a fixed Execution Gas cost of `EXECUTION_SDP_WITHDRAW_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
txhash: zkhash # Mantle transaction hash of the tx containing this operation
withdraw: WithdrawMessage
signature: ZkSignature

ledger: Ledger
service_notes: dict[NoteId, ServiceNote]
declarations: dict[DeclarationID, DeclarationInfo]
```

  *Validate*

  1. Ensure that the service note exists and is bound to this declaration.
      ```python
      assert ledger.is_unspent(withdraw.service_note_id)
      assert withdraw.service_note_id in service_notes

      service_note = service_notes[withdraw.service_note_id]

      assert withdraw.declaration in service_note.declarations
      ```

  2. Validate SDP withdrawal according to [**Withdraw**](bedrock-service-declaration-protocol.md#withdraw).
      1. Ensure declaration exists.
          ```python
          assert withdraw.declaration in declarations
          declare_info = declarations[withdraw.declaration]
          ```
      2. Ensure the declaration is not already scheduled for withdrawal.
          ```python
          assert declare_info.withdraw_at is None
          ```
      3. Ensure service note `pk` and `zk_id` attached to this declaration authorized this Operation.
          ```python
          service_note = ledger[withdraw.service_note_id]
          assert ZkSignature_verify(txhash, signature, [service_note.pk, declare_info.zk_id])
          ```
      4. Ensure that the nonce is greater than the previous one.
          ```python
          assert withdraw.nonce > declare_info.nonce
          ```

#### Execution

  *Given*

```python
withdraw: WithdrawMessage
signature: ZkSignature

current_epoch: EpochNumber # current epoch
ledger: Ledger
service_notes: dict[NoteId, ServiceNote]
declarations: dict[DeclarationID, DeclarationInfo]
```

  *Execute*

  Executes the withdrawal protocol [**Withdraw**](bedrock-service-declaration-protocol.md#withdraw).

  1. Update the declaration info with the nonce and the withdrawal epoch.
      ```python
      declare_info = declarations[withdraw.declaration]
      declare_info.nonce = withdraw.nonce
      declare_info.withdraw_at = current_epoch + 2
      ```

#### Example

```python
withdraw=Withdraw(
    declaration=alice_declaration_id,
    service_note_id=alices_service_note_id
    nonce=1579532
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[alices_service_note_id],
                    outputs=[Note(100, alice_note_pk)])

tx = MantleTx(
    ops=[Op(opcode=SDP_WITHDRAW, payload=encode(withdraw)),
         Op(opcode=TRANSFER, payload=encode(transfer))],
)

SignedMantleTx(
    tx=tx,
    # proof ownership of the withdrawn note and zk id
    op_proofs=[ZkSignature_sign([alice_note_sk, alice_sk], mantle_txhash(tx)),
               transfer.prove(alice_sk)]
)
```

### SDP Epoch Finalization

Withdrawn declarations are removed by Mantle as part of the epoch transition,
not when the `WithdrawMessage` is processed. The rewards of epoch
`withdraw_at - 1` ([Withdraw](bedrock-service-declaration-protocol.md#withdraw))
are distributed in the first block of epoch `withdraw_at + 1` (see
[Service Reward Distribution Protocol](bedrock-service-reward-distribution.md)).
In that same block, after the rewards have been distributed, every declaration
with `withdraw_at + 1 <= current_epoch` is removed and its stake unlocked.
Declarations that withdrew without earning a final reward are removed by the
same step.

  *Given*

```python
current_epoch: EpochNumber
service_notes: dict[NoteId, ServiceNote]
declarations: dict[DeclarationID, DeclarationInfo]
```

  *Execute*

  For every `declare_id`, `declare_info` in `declarations` where
  `declare_info.withdraw_at is not None and declare_info.withdraw_at + 1 <= current_epoch`:

  1. Remove the declaration from its service note.
      ```python
      service_note = service_notes[declare_info.service_note_id]
      service_note.declarations.remove(declare_id)
      ```

  2. Remove the declaration.
      ```python
      del declarations[declare_id]
      ```

  3. Unlock the note once it is no longer bound to any declaration.
      ```python
      if len(service_note.declarations) == 0:
          del service_notes[declare_info.service_note_id]
      ```

### SDP_ACTIVE

The service active action follows the definition given in [Active Message](bedrock-service-declaration-protocol.md#active-message).

#### Payload

```python
class Active:
    declaration: DeclarationID
    nonce: int
    metadata: bytes # a service-specific node activeness metadata
```

#### Proof

```python
ZkSignature
```

#### Execution Gas

  SDP Active Operations have a fixed Execution Gas cost of `EXECUTION_SDP_ACTIVE_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

#### Validation

  *Given*

```python
txhash: zkhash # Mantle transaction hash of the tx containing this operation
active: Active
signature: ZkSignature

declarations: dict[DeclarationID, DeclarationInfo]
```

  *Validate*

```python
assert active.declaration in declarations
declaration_info = declarations[active.declaration]

assert active.nonce > declaration_info.nonce

assert ZkSignature_verify(txhash, signature, declaration_info.zk_id)
```

#### Execution

  *Given*

```python
active: Active

current_epoch: EpochNumber # epoch of the block containing this operation
declarations: dict[DeclarationID, DeclarationInfo]
```

  *Execute*

  Executes the active protocol [Active](bedrock-service-declaration-protocol.md#active). If the service-specific activity logic rejects the message, the Operation is invalid.

  1. Update the declaration info with the nonce and the epoch.
      ```python
      declaration_info = declarations[active.declaration]
      declaration_info.nonce = active.nonce
      declaration_info.active = current_epoch
      ```

#### Example

```python
active=Active(
    declaration=alice_declaration_id,
    nonce=1579532,
    metadata=b"Look, I am still doing my job"
)

# Build the transfer operation to pay the fees
transfer = Transfer(inputs=[fee_note_id], outputs=[])

tx = MantleTx(
    ops=[Op(opcode=SDP_ACTIVE, payload=encode(active))],
)
txhash = mantle_txhash(tx)

SignedMantleTx(
    tx=tx,
    op_proofs=[Ed25519_sign(txhash, validator_sk), transfer.prove(fee_note_sk)]
)
```

