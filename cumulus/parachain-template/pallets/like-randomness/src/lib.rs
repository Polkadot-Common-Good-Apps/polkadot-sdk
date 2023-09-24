#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::pallet_prelude::*;
use frame_support::sp_runtime::traits::Hash;
use frame_support::traits::Randomness;
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;
use scale_info::prelude::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event emitted when a random number is create
		UniqueCreated(u32),
	}

	//Nonce storage item
	#[pallet::storage]
	#[pallet::getter(fn nonce)]
	pub type Nonce<T: Config> = StorageValue<_, u32, ValueQuery>;

	//Random Storage
	#[pallet::storage]
	#[pallet::getter(fn like_random)]
	pub type LikeRandomNumber<T: Config> = StorageValue<_, T::Hash, ValueQuery>;
}

impl<T: Config> Pallet<T> {
	//private function to increment the nonce
	fn get_and_increment_nonce() -> Vec<u8> {
		let nonce = <Nonce<T>>::get();
		<Nonce<T>>::put(nonce.saturating_add(1));
		nonce.encode()
	}
}

impl<T: Config> Randomness<T::Hash, BlockNumberFor<T>> for Pallet<T> {
	// This function is not intended to generate a random number, but to show a prototype of how to use the Randomness trait.
	fn random(subject: &[u8]) -> (T::Hash, BlockNumberFor<T>) {
		let block_number = <frame_system::Pallet<T>>::block_number();
		// Increment Nonce
		let nonce = Self::get_and_increment_nonce();

		// Write the random value to storage.
		let encoded_block: Vec<u8> = block_number.encode();
		let data = [&*encoded_block, &*nonce].concat();
		let seed = T::Hashing::hash_of(&data);

		<LikeRandomNumber<T>>::put(seed);
		(seed, block_number)
	}
}
