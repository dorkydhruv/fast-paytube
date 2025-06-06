```mermaid
sequenceDiagram
    participant UserWallet as User Wallet
    participant SourceSVM as Source SVM Layer (e.g., Solana L1)
    participant EscrowContract as SourcePortalContract (on SourceSVM)
    participant AuthCommittee as Neutral Authority Committee (Off-Chain)
    participant RelayerNet as Relayer Network (Off-Chain)
    participant DestSVM as Destination SVM Layer (e.g., SONIC L2)
    participant BridgeContract as DestinationPortalContract (on DestSVM)

    UserWallet->>+EscrowContract: 1. initiateTransfer(destLayer, destAddr, asset, amount)
    Note over EscrowContract: Locks assets, generates interopTxId
    EscrowContract-->>-UserWallet: Returns interopTxId (or tx confirmation)
    EscrowContract->>AuthCommittee: 2. Emits TransferInitiatedEvent (interopTxId, details...)

    Note over AuthCommittee: Each Authority Node:
    Note over AuthCommittee: - Listens for event
    Note over AuthCommittee: - Internal shard processes specific interopTxId
    Note over AuthCommittee: - Verifies lock on SourceSVM
    Note over AuthCommittee: - Participates in BFT Threshold Signature

    AuthCommittee-->>RelayerNet: 3. Attestation Created (interopTxId, details, threshold_signature)
    Note over RelayerNet: Relayer picks up Attestation

    RelayerNet->>+BridgeContract: 4. completeTransfer(Attestation)
    Note over BridgeContract: - Verifies Attestation (auth. signatures, replay)
    Note over BridgeContract: - Mints wrapped asset / Unlocks asset
    BridgeContract-->>-RelayerNet: Returns tx confirmation
    Note over UserWallet, DestSVM: User receives assets on Destination SVM Layer
```

Usecase:


generate-config - 

```bash
cargo run -- generate-config --num-authorities 4 --num-shards 16 --output-dir ./bridge_config
```

run servers -

```bash
chmod +x ./scipts/start_auth.sh
./scripts/start_auth.sh
```

run relayer -

```bash
cargo run -- relayer --committee ./bridge_config/committee.json --source-rpc 'http://localhost:3001' --destination-rpc 'http://localhost:3000'
```
