## Mantle Ledger

The Mantle Ledger enables asset transfers using a transparent UTXO model. While a Transfer Operation can consume more tokens than it creates, the Mantle Transaction excess balance must exactly pay for the fees. The ledger tracks three kinds of notes: regular notes, service notes (collateral for service declarations) and channel notes (channel bridge funds eligible for PoS participation only).

# Mantle Ledger

## Notes

Notes are composed of two fields representing their value and their owner:

```python
class Note:
    value: TokenValue   # uint64
    public_key: ZkPublicKey # 32 bytes
```

### Note Id

A note can be uniquely identified by the Operation that created it and its output number: `(op_id, output_number)` if each Operation are uniquely identifiable. For this reason, every Operation that output notes have a unique payload that is used to derive the Operation identifier. Because it is often useful to have a commitment to the note fields for use in ZK proofs (e.g., for PoL), we included the note in the note identifier derivation.

```python
def derive_op_id(operation: Op) -> Hash:
    op_bytes = encode(op)
    h = Hasher() # /!\ This is a classic hash not a zkhash /!\
    h.update(b"OPERATION_ID_V1")
    h.update(op_bytes)
    return h.digest()

def derive_note_id(op_id: Hash, output_number: int, note: Note) -> NoteId:
    return zkhash(
        FiniteField(b"NOTE_ID_V1", byte_order="little", modulus= p),
        FiniteField(op_id, byte_order="little", modulus= p),
        FiniteField(output_number, byte_order="little", modulus= p),
        FiniteField(note.value, byte_order="little", modulus= p),
        note.public_key
    )
```

`op_id` is a classical 256-bit hash digest and must be reduced to a field element before being passed to the ZkHasher. We apply a direct modular reduction mod `p` (via `FiniteField(..., modulus=p)`). Since $`p \approx2^{-254}`$, the reduction is slightly non-uniform, values in $`[0, 2^{256} \mod p)`$ appear one extra time, but this is inconsequential in practice: the collision probability remains around $`2^{-254}`$, and `NoteId` uniqueness is not derived from uniformity of `op_id` over $`𝔽_p`$ but from the collision-resistance of the underlying hash and per-operation payload uniqueness.

These note identifiers uniquely define notes in the system and cannot be chosen by the user. Nodes maintain the set of notes through a dictionary mapping the NoteId to the note.

### Service notes

Service notes are special notes in Mantle that serve as collateral for Service Declarations. A note can become a service note after being locked by executing a Declare Operation, preventing it from being spent until explicitly released through a Withdraw Operation. The system maintains a mapping of service note IDs to their supporting declarations. Though locked, these notes remain in the Ledger and can still participate in Proof of Stake. When service providers withdraw all their declarations, the associated note(s) become unlocked and available for spending again.

### Channel Notes

Channel notes are on-ledger notes minted to represent channel funds. They are distinct from Service Notes as they can’t be used to declare a service. However, they follow the same ageing rule as ordinary notes since they are part of the ledger and can be used for PoL creation once aged enough.

The system maintains a `channel_notes` set in the Ledger tracking all active channel `NoteId` and their respective `ChannelId`.

## Ledger

```python
class Ledger:
    notes: list[Note]
    service_notes: dict[NoteId, ServiceNote]
    channel_notes: dict[NoteId, ChannelId]
```

### Input Notes Spendability Validation

A note is spendable if and only if it exists, it is not spent or a service note. The following function validates that an input of notes can be consumed:

```python
class Ledger:
    def assert_spendable(inputs: list[NoteId], channel_id: ChannelId | None):
		# Assert inputs are empty
		assert len(inputs) > 0

        ## Check there is no duplicate
        assert len(inputs) == len(set(inputs))

            # Check that each note is individualy not a service note, for the correct channel and unspent
            for note_id in inputs:
                assert ledger.is_unspent(note_id)
                assert note_id not in service_notes
                if channel_id is None:
                	assert note_id not in ledger.channel_notes
                else:
                	assert note_id in ledger.channel_notes
                    assert ledger.channel_notes[note_id] == channel_id
```

### Output Notes Validation

Before an output of notes can be inserted into the Ledger, every note field must satisfy the following constraints:

```python
class Ledger:
    def assert_valid_output(outputs: list[Note]):
        for note in outputs:
            assert note.value > 0
            assert note.value <= 2**64-1
```

### Consuming Input Notes Execution

Consuming a set of notes removes them from the Ledger’s Merkle tree and recycles their leaf indices:

```python
class Ledger:
    def execute_spending(inputs: list[NoteId], channel_id: ChannelId | None):
        for note_id in inputs:
            # updates the merkle tree to zero out the leaf for this entry
            # and adds that leaf index to the list of unused leaves
            ledger.remove(note_id)
            if channel_id is not None:
                ledger.channel_notes.pop(note_id)
```

### Creating Output Notes Execution

Creating notes derives their `NoteId` from the Operation’s `OpId` and insert them in the Ledger:

```python
class Ledger:
    def execute_adding(op_id: Hash, outputs: list[Note], channel_id: ChannelId | None):
        for (output_index, output_note) in enumerate(outputs):
            output_note_id = derive_note_id(op_id, output_index, output_note)
            ledger.add(output_note_id)
            if channel_id is not None:
                ledger.channel_notes[output_note_id] = channel_id
```

