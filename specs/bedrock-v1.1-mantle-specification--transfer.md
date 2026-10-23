## TRANSFER

Transactions must prove the ownership of spent notes. In classical blockchains, this is done through a signature. To stay compatible with our architecture, the signature is done by a ZK proof (see [Zero Knowledge Signature Scheme (ZkSignature)](#zero-knowledge-signature-scheme-zksignature)), proving the knowledge of the secret key associated with the public key.

Transactions allow complete transaction linkability and the public key spending the note is not hidden.

### Payload

```python
class Transfer:
    inputs: list[NoteId]  # the list of consumed note identifiers
                          # must be non-empty
    outputs: list[Note]
```

### Proof

  A Transfer proves the ownership of the consumed notes using a [Zero Knowledge Signature Scheme (ZkSignature)](#zero-knowledge-signature-scheme-zksignature).

```python
ZkSignature
```

### Execution Gas

  Transfer have a fixed Execution Gas cost of `EXECUTION_TRANSFER_GAS`. See [Gas Determination](#gas-determination) for the Execution Gas values.

### Validation

  *Given*

```python
mantle_txhash: zkhash # zkhash of mantle tx containing this ledger tx
transfer: Transfer
transfer_proof: ZkSignature

ledger: Ledger
```

  *Validate*

  1. Ensure all inputs are spendable and not in a channel.
      ```python
      ledger.assert_spendable(transfer.inputs)
      ```

  2. Validate transfer proof to show ownership over input notes.
      ```python
      input_notes = [ledger[input_note_id] for input_note_id in transfer.inputs]
      input_pks = [note.public_key for note in input_notes]
      assert ZkSignature_verify(mantle_txhash, transfer_proof, input_pks)
      ```

  3. Ensure outputs are valid.
      ```python
      ledger.assert_valid_output(transfer.output)
      ```

### Execution

  *Given*

```python
transfer: Transfer
transfer_proof: ZkSignature

ledger: Ledger
```

  *Execution*

  1. Remove inputs from the ledger.
      ```python
      ledger.execute_spending(transfer.inputs)
      ```

  2. Add outputs to the ledger.
      ```python
      transfer_id = derive_operation_id(transfer)
      ledger.execute_adding(transfer_id, transfer.outputs)
      ```

### Example

```python
alice_note_id = ... # assume Alice holds a note worth 501 tokens
bob_note=Note(
    value=500
    public_key=bob_pk,
)

transfer = Transfer(
    inputs=[alice_note_id],
    outputs=[bob_note],
)
```

