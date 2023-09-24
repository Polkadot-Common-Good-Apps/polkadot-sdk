### Polkadot Common Good Apps

This is a PoC/MVP of the Common Good Parachain intended to allocate Common good apps.

In this fork of the Polkadot SDK, the parachain template located here:

https://github.com/Polkadot-Common-Good-Apps/polkadot-sdk/tree/master/cumulus/parachain-template

it was modified to fulfil some ideas of how the first application being a lottery will be configured in the runtime.

## Custom pallets

### LikeRandomess
https://github.com/Polkadot-Common-Good-Apps/polkadot-sdk/tree/master/cumulus/parachain-template/pallets/like-randomness

This pallet is a prototype that implements the necessary `Randomness` trait so it can be provided to the `Randonmess` type of the Lottery pallet. Naturally the randomness is not being generated in this pallet and it simulates providing one.

### Lottery:
https://github.com/Polkadot-Common-Good-Apps/polkadot-sdk/tree/master/cumulus/parachain-template/pallets/lottery

This is a fork of the existing Lottery pallet from FRAME:
https://github.com/Polkadot-Common-Good-Apps/polkadot-sdk/tree/master/substrate/frame/lottery

This pallet was tightly coupling with the `pallet_xcm` in order to be able to teleport the funds allocated in this pallet into the treasury account from the relay chain. Also the treasury address (in AccountId32 format) was added as a `RelayChainTreasuryAccount` Config type for this forked pallet.