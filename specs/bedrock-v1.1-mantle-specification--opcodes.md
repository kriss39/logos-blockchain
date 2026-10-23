## Opcodes

| **Operation** | **Opcode** | **Description** |
| --- | --- | --- |
| TRANSFER | 0x00 | Consume and create notes. |
| *RESERVED* | *0x01 - 0x0F* |  |
| CHANNEL_CONFIG | 0x10 | Configure a channel |
| CHANNEL_INSCRIBE | 0x11 | Write a message permanently onto Mantle. |
| CHANNEL_DEPOSIT | 0x12 | Deposit assets into a channel |
| CHANNEL_WITHDRAW | 0x13 | Withdraw assets from a channel |
| CHANNEL_TRANSFER | 0x14 | Consume and create notes belonging to a channel |
| *RESERVED* | *0x15 - 0x1F* |  |
| SDP_DECLARE | 0x20 | Declare intention to participate as a node in a Bedrock Service, locking funds as collateral. |
| SDP_WITHDRAW | 0x21 | Withdraw participation from a Bedrock Service, unlocking your funds in the process. |
| SDP_ACTIVE | 0x22 | Signal that you are still an active participant of a Bedrock Service. |
| *RESERVED* | *0x23 - 0x2F* |  |
| LEADER_CLAIM | 0x30 | Claim leader reward anonymously. |
| *RESERVED* | *0x31 - 0x3F* |  |
| CLAIM_POW_REWARD | 0x40 | Claim a reward from the pow reward pool. |
| *RESERVED* | *0x41 - 0xFF* |  |

