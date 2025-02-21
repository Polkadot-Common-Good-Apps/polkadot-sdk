// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	configuration::Pallet as Configuration,
	hrmp::{Pallet as Hrmp, *},
	paras::{Pallet as Paras, ParaKind, ParachainsCache},
	shared::Pallet as Shared,
};
use frame_benchmarking::{impl_benchmark_test_suite, v2::*, whitelisted_caller};
use frame_support::{assert_ok, traits::Currency};

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

fn register_parachain_with_balance<T: Config>(id: ParaId, balance: BalanceOf<T>) {
	let mut parachains = ParachainsCache::new();
	Paras::<T>::initialize_para_now(
		&mut parachains,
		id,
		&crate::paras::ParaGenesisArgs {
			para_kind: ParaKind::Parachain,
			genesis_head: vec![1].into(),
			validation_code: vec![1].into(),
		},
	);
	T::Currency::make_free_balance_be(&id.into_account_truncating(), balance);
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

/// Enumerates the phase in the setup process of a channel between two parachains.
enum ParachainSetupStep {
	/// A channel open has been requested
	Requested,
	/// A channel open has been requested and accepted.
	Accepted,
	/// A channel open has been requested and accepted, and a session has passed and is now open.
	Established,
	/// A channel open has been requested and accepted, and a session has passed and is now
	/// open, and now it has been requested to close down.
	CloseRequested,
}

fn establish_para_connection<T: Config>(
	from: u32,
	to: u32,
	until: ParachainSetupStep,
) -> [(ParaId, crate::Origin); 2]
where
	<T as frame_system::Config>::RuntimeOrigin: From<crate::Origin>,
{
	let config = Configuration::<T>::config();
	let ed = T::Currency::minimum_balance();
	let deposit: BalanceOf<T> = config.hrmp_sender_deposit.unique_saturated_into();
	let capacity = config.hrmp_channel_max_capacity;
	let message_size = config.hrmp_channel_max_message_size;

	let sender: ParaId = from.into();
	let sender_origin: crate::Origin = from.into();

	let recipient: ParaId = to.into();
	let recipient_origin: crate::Origin = to.into();

	let output = [(sender, sender_origin.clone()), (recipient, recipient_origin.clone())];

	// Make both a parachain if they are already not.
	if !Paras::<T>::is_parachain(sender) {
		register_parachain_with_balance::<T>(sender, deposit + ed);
	}
	if !Paras::<T>::is_parachain(recipient) {
		register_parachain_with_balance::<T>(recipient, deposit + ed);
	}

	assert_ok!(Hrmp::<T>::hrmp_init_open_channel(
		sender_origin.clone().into(),
		recipient,
		capacity,
		message_size
	));

	if matches!(until, ParachainSetupStep::Requested) {
		return output
	}

	assert_ok!(Hrmp::<T>::hrmp_accept_open_channel(recipient_origin.into(), sender));
	if matches!(until, ParachainSetupStep::Accepted) {
		return output
	}

	Hrmp::<T>::process_hrmp_open_channel_requests(&Configuration::<T>::config());
	if matches!(until, ParachainSetupStep::Established) {
		return output
	}

	let channel_id = HrmpChannelId { sender, recipient };
	assert_ok!(Hrmp::<T>::hrmp_close_channel(sender_origin.clone().into(), channel_id));
	if matches!(until, ParachainSetupStep::CloseRequested) {
		// NOTE: this is just for expressiveness, otherwise the if-statement is 100% useless.
		return output
	}

	output
}

/// Prefix value for account generation. These numbers are used as seeds to create distinct (para)
/// accounts.
///
/// To maintain sensibility created accounts should always be unique and never overlap. For example,
/// if for some benchmarking component `c`, accounts are being created `for s in 0..c` with seed
/// `PREFIX_0 + s`, then we must assert that `c <= PREFIX_1`, meaning that it won't overlap with
/// `PREFIX_2`.
///
/// These values are chosen large enough so that the likelihood of any clash is already very low.
const PREFIX_0: u32 = 10_000;
const PREFIX_1: u32 = PREFIX_0 * 2;
const MAX_UNIQUE_CHANNELS: u32 = 128;

static_assertions::const_assert!(MAX_UNIQUE_CHANNELS < PREFIX_0);
static_assertions::const_assert!(HRMP_MAX_INBOUND_CHANNELS_BOUND < PREFIX_0);
static_assertions::const_assert!(HRMP_MAX_OUTBOUND_CHANNELS_BOUND < PREFIX_0);

#[benchmarks(where <T as frame_system::Config>::RuntimeOrigin: From<crate::Origin>)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn hrmp_init_open_channel() {
		let sender_id: ParaId = 1u32.into();
		let sender_origin: crate::Origin = 1u32.into();

		let recipient_id: ParaId = 2u32.into();

		// make sure para is registered, and has enough balance.
		let ed = T::Currency::minimum_balance();
		let deposit: BalanceOf<T> =
			Configuration::<T>::config().hrmp_sender_deposit.unique_saturated_into();
		register_parachain_with_balance::<T>(sender_id, deposit + ed);
		register_parachain_with_balance::<T>(recipient_id, deposit + ed);

		let capacity = Configuration::<T>::config().hrmp_channel_max_capacity;
		let message_size = Configuration::<T>::config().hrmp_channel_max_message_size;

		#[extrinsic_call]
		_(sender_origin, recipient_id, capacity, message_size);

		assert_last_event::<T>(
			Event::<T>::OpenChannelRequested(sender_id, recipient_id, capacity, message_size)
				.into(),
		);
	}

	#[benchmark]
	fn hrmp_accept_open_channel() {
		let [(sender, _), (recipient, recipient_origin)] =
			establish_para_connection::<T>(1, 2, ParachainSetupStep::Requested);

		#[extrinsic_call]
		_(recipient_origin, sender);

		assert_last_event::<T>(Event::<T>::OpenChannelAccepted(sender, recipient).into());
	}

	#[benchmark]
	fn hrmp_close_channel() {
		let [(sender, sender_origin), (recipient, _)] =
			establish_para_connection::<T>(1, 2, ParachainSetupStep::Established);
		let channel_id = HrmpChannelId { sender, recipient };

		#[extrinsic_call]
		_(sender_origin, channel_id.clone());

		assert_last_event::<T>(Event::<T>::ChannelClosed(sender, channel_id).into());
	}

	// NOTE: a single parachain should have the maximum number of allowed ingress and egress
	// channels.
	#[benchmark]
	fn force_clean_hrmp(
		// ingress channels to a single leaving parachain that need to be closed.
		i: Linear<0, { HRMP_MAX_INBOUND_CHANNELS_BOUND - 1 }>,
		// egress channels to a single leaving parachain that need to be closed.
		e: Linear<0, { HRMP_MAX_OUTBOUND_CHANNELS_BOUND - 1 }>,
	) {
		// first, update the configs to support this many open channels...
		assert_ok!(Configuration::<T>::set_hrmp_max_parachain_outbound_channels(
			frame_system::RawOrigin::Root.into(),
			e + 1
		));
		assert_ok!(Configuration::<T>::set_hrmp_max_parachain_inbound_channels(
			frame_system::RawOrigin::Root.into(),
			i + 1
		));
		assert_ok!(Configuration::<T>::set_max_downward_message_size(
			frame_system::RawOrigin::Root.into(),
			1024
		));
		// .. and enact it.
		Configuration::<T>::initializer_on_new_session(&Shared::<T>::scheduled_session());

		let config = Configuration::<T>::config();
		let deposit: BalanceOf<T> = config.hrmp_sender_deposit.unique_saturated_into();

		let para: ParaId = 1u32.into();
		register_parachain_with_balance::<T>(para, deposit);
		T::Currency::make_free_balance_be(&para.into_account_truncating(), deposit * 256u32.into());

		for ingress_para_id in 0..i {
			// establish ingress channels to `para`.
			let ingress_para_id = ingress_para_id + PREFIX_0;
			let _ = establish_para_connection::<T>(
				ingress_para_id,
				para.into(),
				ParachainSetupStep::Established,
			);
		}

		// nothing should be left unprocessed.
		assert_eq!(HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default(), 0);

		for egress_para_id in 0..e {
			// establish egress channels to `para`.
			let egress_para_id = egress_para_id + PREFIX_1;
			let _ = establish_para_connection::<T>(
				para.into(),
				egress_para_id,
				ParachainSetupStep::Established,
			);
		}

		// nothing should be left unprocessed.
		assert_eq!(HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default(), 0);

		// all in all, we have created this many channels.
		assert_eq!(HrmpChannels::<T>::iter().count() as u32, i + e);

		#[extrinsic_call]
		_(frame_system::Origin::<T>::Root, para, i, e);

		// all in all, all of them must be gone by now.
		assert_eq!(HrmpChannels::<T>::iter().count() as u32, 0);
		// borrow this function from the tests to make sure state is clear, given that we do a lot
		// of out-of-ordinary ops here.
		Hrmp::<T>::assert_storage_consistency_exhaustive();
	}

	#[benchmark]
	fn force_process_hrmp_open(
		// number of channels that need to be processed. Worse case is an N-M relation: unique
		// sender and recipients for all channels.
		c: Linear<0, MAX_UNIQUE_CHANNELS>,
	) {
		for id in 0..c {
			let _ = establish_para_connection::<T>(
				PREFIX_0 + id,
				PREFIX_1 + id,
				ParachainSetupStep::Accepted,
			);
		}
		assert_eq!(HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32, c);

		#[extrinsic_call]
		_(frame_system::Origin::<T>::Root, c);

		assert_eq!(HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32, 0);
	}

	#[benchmark]
	fn force_process_hrmp_close(
		// number of channels that need to be processed. Worse case is an N-M relation: unique
		// sender and recipients for all channels.
		c: Linear<0, MAX_UNIQUE_CHANNELS>,
	) {
		for id in 0..c {
			let _ = establish_para_connection::<T>(
				PREFIX_0 + id,
				PREFIX_1 + id,
				ParachainSetupStep::CloseRequested,
			);
		}

		assert_eq!(HrmpCloseChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32, c);

		#[extrinsic_call]
		_(frame_system::Origin::<T>::Root, c);

		assert_eq!(HrmpCloseChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32, 0);
	}

	#[benchmark]
	fn hrmp_cancel_open_request(
		// number of items already existing in the `HrmpOpenChannelRequestsList`, other than the
		// one that we remove.
		c: Linear<0, MAX_UNIQUE_CHANNELS>,
	) {
		for id in 0..c {
			let _ = establish_para_connection::<T>(
				PREFIX_0 + id,
				PREFIX_1 + id,
				ParachainSetupStep::Requested,
			);
		}

		let [(sender, sender_origin), (recipient, _)] =
			establish_para_connection::<T>(1, 2, ParachainSetupStep::Requested);
		assert_eq!(
			HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32,
			c + 1
		);
		let channel_id = HrmpChannelId { sender, recipient };

		#[extrinsic_call]
		_(sender_origin, channel_id, c + 1);

		assert_eq!(HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32, c);
	}

	// worse case will be `n` parachain channel requests, where in all of them either the sender or
	// the recipient need to be cleaned. This enforces the deposit of at least one to be processed.
	// No code path for triggering two deposit process exists.
	#[benchmark]
	fn clean_open_channel_requests(c: Linear<0, MAX_UNIQUE_CHANNELS>) {
		for id in 0..c {
			let _ = establish_para_connection::<T>(
				PREFIX_0 + id,
				PREFIX_1 + id,
				ParachainSetupStep::Requested,
			);
		}

		assert_eq!(HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32, c);
		let outgoing = (0..c).map(|id| (id + PREFIX_1).into()).collect::<Vec<ParaId>>();
		let config = Configuration::<T>::config();

		#[block]
		{
			Hrmp::<T>::clean_open_channel_requests(&config, &outgoing);
		}

		assert_eq!(HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32, 0);
	}

	#[benchmark]
	fn force_open_hrmp_channel(
		// Weight parameter only accepts `u32`, `0` and `1` used to represent `false` and `true`,
		// respectively.
		c: Linear<0, 1>,
	) {
		let sender_id: ParaId = 1u32.into();
		let sender_origin: crate::Origin = 1u32.into();
		let recipient_id: ParaId = 2u32.into();

		// Make sure para is registered. The sender does actually need the normal deposit because it
		// is first going the "init" route.
		let ed = T::Currency::minimum_balance();
		let sender_deposit: BalanceOf<T> =
			Configuration::<T>::config().hrmp_sender_deposit.unique_saturated_into();
		register_parachain_with_balance::<T>(sender_id, sender_deposit + ed);
		register_parachain_with_balance::<T>(recipient_id, Zero::zero());

		let capacity = Configuration::<T>::config().hrmp_channel_max_capacity;
		let message_size = Configuration::<T>::config().hrmp_channel_max_message_size;

		let channel_id = HrmpChannelId { sender: sender_id, recipient: recipient_id };
		if c == 1 {
			// this will consume more weight if a channel _request_ already exists, because it
			// will need to clear the request.
			assert_ok!(Hrmp::<T>::hrmp_init_open_channel(
				sender_origin.clone().into(),
				recipient_id,
				capacity,
				message_size
			));
			assert!(HrmpOpenChannelRequests::<T>::get(&channel_id).is_some());
		} else {
			if HrmpOpenChannelRequests::<T>::get(&channel_id).is_some() {
				assert_ok!(Hrmp::<T>::hrmp_cancel_open_request(
					sender_origin.clone().into(),
					channel_id.clone(),
					MAX_UNIQUE_CHANNELS,
				));
			}
			assert!(HrmpOpenChannelRequests::<T>::get(&channel_id).is_none());
		}

		// but the _channel_ should not exist
		assert!(HrmpChannels::<T>::get(&channel_id).is_none());

		#[extrinsic_call]
		_(frame_system::Origin::<T>::Root, sender_id, recipient_id, capacity, message_size);

		assert_last_event::<T>(
			Event::<T>::HrmpChannelForceOpened(sender_id, recipient_id, capacity, message_size)
				.into(),
		);
	}

	#[benchmark]
	fn establish_system_channel() {
		let sender_id: ParaId = 1u32.into();
		let recipient_id: ParaId = 2u32.into();

		let caller: T::AccountId = whitelisted_caller();
		let config = Configuration::<T>::config();

		// make sure para is registered, and has zero balance.
		register_parachain_with_balance::<T>(sender_id, Zero::zero());
		register_parachain_with_balance::<T>(recipient_id, Zero::zero());

		let capacity = config.hrmp_channel_max_capacity;
		let message_size = config.hrmp_channel_max_message_size;

		#[extrinsic_call]
		_(frame_system::RawOrigin::Signed(caller), sender_id, recipient_id);

		assert_last_event::<T>(
			Event::<T>::HrmpSystemChannelOpened(sender_id, recipient_id, capacity, message_size)
				.into(),
		);
	}

	#[benchmark]
	fn poke_channel_deposits() {
		let sender_id: ParaId = 1u32.into();
		let recipient_id: ParaId = 2u32.into();
		let channel_id = HrmpChannelId { sender: sender_id, recipient: recipient_id };

		let caller: T::AccountId = whitelisted_caller();
		let config = Configuration::<T>::config();

		// make sure para is registered, and has balance to reserve.
		let ed = T::Currency::minimum_balance();
		let sender_deposit: BalanceOf<T> = config.hrmp_sender_deposit.unique_saturated_into();
		let recipient_deposit: BalanceOf<T> = config.hrmp_recipient_deposit.unique_saturated_into();
		register_parachain_with_balance::<T>(sender_id, ed + sender_deposit);
		register_parachain_with_balance::<T>(recipient_id, ed + recipient_deposit);

		// Our normal establishment won't actually reserve deposits, so just insert them directly.
		HrmpChannels::<T>::insert(
			&channel_id,
			HrmpChannel {
				sender_deposit: config.hrmp_sender_deposit,
				recipient_deposit: config.hrmp_recipient_deposit,
				max_capacity: config.hrmp_channel_max_capacity,
				max_total_size: config.hrmp_channel_max_total_size,
				max_message_size: config.hrmp_channel_max_message_size,
				msg_count: 0,
				total_size: 0,
				mqc_head: None,
			},
		);
		// Actually reserve the deposits.
		let _ = T::Currency::reserve(&sender_id.into_account_truncating(), sender_deposit);
		let _ = T::Currency::reserve(&recipient_id.into_account_truncating(), recipient_deposit);

		#[extrinsic_call]
		_(frame_system::RawOrigin::Signed(caller), sender_id, recipient_id);

		assert_last_event::<T>(
			Event::<T>::OpenChannelDepositsUpdated(sender_id, recipient_id).into(),
		);
		let channel = HrmpChannels::<T>::get(&channel_id).unwrap();
		// Check that the deposit was updated in the channel state.
		assert_eq!(channel.sender_deposit, 0);
		assert_eq!(channel.recipient_deposit, 0);
		// And that the funds were unreserved.
		assert_eq!(
			T::Currency::reserved_balance(&sender_id.into_account_truncating()),
			0u128.unique_saturated_into()
		);
		assert_eq!(
			T::Currency::reserved_balance(&recipient_id.into_account_truncating()),
			0u128.unique_saturated_into()
		);
	}

	impl_benchmark_test_suite!(
		Hrmp,
		crate::mock::new_test_ext(crate::hrmp::tests::GenesisConfigBuilder::default().build()),
		crate::mock::Test
	);
}
