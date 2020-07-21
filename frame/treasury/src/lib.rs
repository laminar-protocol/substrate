// This file is part of Substrate.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Treasury Module
//!
//! The Treasury module provides a "pot" of funds that can be managed by stakeholders in the
//! system and a structure for making spending proposals from this pot.
//!
//! - [`treasury::Trait`](./trait.Trait.html)
//! - [`Call`](./enum.Call.html)
//!
//! ## Overview
//!
//! The Treasury Module itself provides the pot to store funds, and a means for stakeholders to
//! propose, approve, and deny expenditures. The chain will need to provide a method (e.g.
//! inflation, fees) for collecting funds.
//!
//! By way of example, the Council could vote to fund the Treasury with a portion of the block
//! reward and use the funds to pay developers.
//!
//! ### Tipping
//!
//! A separate subsystem exists to allow for an agile "tipping" process, whereby a reward may be
//! given without first having a pre-determined stakeholder group come to consensus on how much
//! should be paid.
//!
//! A group of `Tippers` is determined through the config `Trait`. After half of these have declared
//! some amount that they believe a particular reported reason deserves, then a countdown period is
//! entered where any remaining members can declare their tip amounts also. After the close of the
//! countdown period, the median of all declared tips is paid to the reported beneficiary, along
//! with any finders fee, in case of a public (and bonded) original report.
//!
//! ### Bounty
//!
//! A Bounty Spending is a reward for a specified body of work - or specified set of objectives - that
//! needs to be executed for a predefined Treasury amount to be paid out. A Curator is assigned to be delegated
//! the responsibility of assigning a payout address once the specified set of objectives is completed.
//!
//! After the Council has activated a bounty, it delegates the work that requires expertise to the Curator. They
//! get to close the Active bounty. Closing the Active bounty enacts a delayed payout to the payout address. The
//! delay allows for intervention through regular democracy.
//!
//!
//! ### Terminology
//!
//! - **Proposal:** A suggestion to allocate funds from the pot to a beneficiary.
//! - **Beneficiary:** An account who will receive the funds from a proposal iff
//! the proposal is approved.
//! - **Deposit:** Funds that a proposer must lock when making a proposal. The
//! deposit will be returned or slashed if the proposal is approved or rejected
//! respectively.
//! - **Pot:** Unspent funds accumulated by the treasury module.
//!
//! Tipping protocol:
//! - **Tipping:** The process of gathering declarations of amounts to tip and taking the median
//!   amount to be transferred from the treasury to a beneficiary account.
//! - **Tip Reason:** The reason for a tip; generally a URL which embodies or explains why a
//!   particular individual (identified by an account ID) is worthy of a recognition by the
//!   treasury.
//! - **Finder:** The original public reporter of some reason for tipping.
//! - **Finders Fee:** Some proportion of the tip amount that is paid to the reporter of the tip,
//!   rather than the main beneficiary.
//!
//! Bounty:
//! - **Bounty spending proposal:** A proposal to reward a predefined body of work upon completion by the Treasury.
//! - **Proposer:** An account proposing a bounty spending.
//! - **Curator:** An account managing the bounty and assigning a payout address receiving the reward for the completion of work.
//! - **Deposit:** The amount held on deposit for placing a bounty proposal plus the amount held on deposit per byte within the bounty description.
//! - **Bounty value:** The total amount that should be paid to the Payout Address if the bounty is rewarded.
//! - **Payout address:** The account to which the total or part of the bounty is assigned to.
//! - **Payout Delay:** The delay period for which a bounty beneficiary needs to wait before claiming.
//! - **Sub-bounty:** A portion of the total Bounty value assigned to the completion of a specified body of work in the bounty.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! General spending/proposal protocol:
//! - `propose_spend` - Make a spending proposal and stake the required deposit.
//! - `set_pot` - Set the spendable balance of funds.
//! - `configure` - Configure the module's proposal requirements.
//! - `reject_proposal` - Reject a proposal, slashing the deposit.
//! - `approve_proposal` - Accept the proposal, returning the deposit.
//!
//! Tipping protocol:
//! - `report_awesome` - Report something worthy of a tip and register for a finders fee.
//! - `retract_tip` - Retract a previous (finders fee registered) report.
//! - `tip_new` - Report an item worthy of a tip and declare a specific amount to tip.
//! - `tip` - Declare or redeclare an amount to tip for a particular reason.
//! - `close_tip` - Close and pay out a tip.
//!
//! Bounty protocol:
//! - `propose_bounty` - Propose a specific treasury amount to be earmarked for a predefined set of tasks and stake the required deposit.
//! - `create_sub_bounty` - Allocate a portion of the earmarked treasury amount for a parent_bounty_id to a new curator.
//! - `reject_bounty` - Reject a specific treasury amount to be earmarked for a predefined body of work.
//! - `approve_bounty` - Accept a specific treasury amount to be earmarked for a predefined body of work.
//! - `award_bounty` - Close and pay out the specified amount for the completed work.
//! - `claim_bounty` - Claim a specific bounty amount from the Payout Address
//! - `cancel_bounty` - Cancel the earmark for a specific treasury amount and close the bounty.
//! - `extend_bounty_expiry` - Extend the expiry block number of the bounty and stay active.
//!
//! ## GenesisConfig
//!
//! The Treasury module depends on the [`GenesisConfig`](./struct.GenesisConfig.html).

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
use serde::{Serialize, Deserialize};
use sp_std::prelude::*;
use frame_support::{decl_module, decl_storage, decl_event, ensure, print, decl_error, Parameter};
use frame_support::traits::{
	Currency, Get, Imbalance, OnUnbalanced, ExistenceRequirement::{KeepAlive, AllowDeath},
	ReservableCurrency, WithdrawReason
};
use sp_runtime::{Permill, ModuleId, Percent, RuntimeDebug, DispatchResult, traits::{
	Zero, StaticLookup, AccountIdConversion, Saturating, Hash, BadOrigin
}};
use frame_support::weights::{Weight, DispatchClass};
use frame_support::traits::{Contains, ContainsLengthBound, EnsureOrigin};
use codec::{Encode, Decode};
use frame_system::{self as system, ensure_signed};

mod tests;
mod benchmarking;

type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;
type PositiveImbalanceOf<T> = <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::PositiveImbalance;
type NegativeImbalanceOf<T> = <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::NegativeImbalance;

pub trait WeightInfo {
	fn propose_spend() -> Weight;
	fn reject_proposal() -> Weight;
	fn approve_proposal() -> Weight;
	fn report_awesome(r: u32, ) -> Weight;
	fn retract_tip(r: u32, ) -> Weight;
	fn tip_new(r: u32, t: u32, ) -> Weight;
	fn tip(t: u32, ) -> Weight;
	fn close_tip(t: u32, ) -> Weight;
	fn propose_bounty(r: u32, ) -> Weight;
	fn create_sub_bounty(r: u32, ) -> Weight;
	fn approve_bounty() -> Weight;
	fn reject_bounty() -> Weight;
	fn award_bounty() -> Weight;
	fn claim_bounty() -> Weight;
	fn cancel_bounty() -> Weight;
	fn extend_bounty_expiry() -> Weight;
	fn update_bounty_value_minimum() -> Weight;
	fn on_initialize_proposals(p: u32, ) -> Weight;
	fn on_initialize_bounties(b: u32, ) -> Weight;
}

impl WeightInfo for () {
	fn propose_spend() -> Weight { 1_000_000_000 }
	fn reject_proposal() -> Weight { 1_000_000_000 }
	fn approve_proposal() -> Weight { 1_000_000_000 }
	fn report_awesome(_r: u32, ) -> Weight { 1_000_000_000 }
	fn retract_tip(_r: u32, ) -> Weight { 1_000_000_000 }
	fn tip_new(_r: u32, _t: u32, ) -> Weight { 1_000_000_000 }
	fn tip(_t: u32, ) -> Weight { 1_000_000_000 }
	fn close_tip(_t: u32, ) -> Weight { 1_000_000_000 }
	fn propose_bounty(_r: u32, ) -> Weight { 1_000_000_000 }
	fn create_sub_bounty(_r: u32, ) -> Weight { 1_000_000_000 }
	fn approve_bounty() -> Weight { 1_000_000_000 }
	fn reject_bounty() -> Weight { 1_000_000_000 }
	fn award_bounty() -> Weight { 1_000_000_000 }
	fn claim_bounty() -> Weight { 1_000_000_000 }
	fn cancel_bounty() -> Weight { 1_000_000_000 }
	fn extend_bounty_expiry() -> Weight { 1_000_000_000 }
	fn update_bounty_value_minimum() -> Weight { 1_000_000_000 }
	fn on_initialize_proposals(_p: u32, ) -> Weight { 1_000_000_000 }
	fn on_initialize_bounties(_b: u32, ) -> Weight { 1_000_000_000 }
}

pub trait Trait: frame_system::Trait {
	/// The treasury's module id, used for deriving its sovereign account ID.
	type ModuleId: Get<ModuleId>;

	/// The staking balance.
	type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

	/// Origin from which approvals must come.
	type ApproveOrigin: EnsureOrigin<Self::Origin>;

	/// Origin from which rejections must come.
	type RejectOrigin: EnsureOrigin<Self::Origin>;

	/// Origin from which tippers must come.
	///
	/// `ContainsLengthBound::max_len` must be cost free (i.e. no storage read or heavy operation).
	type Tippers: Contains<Self::AccountId> + ContainsLengthBound;

	/// The period for which a tip remains open after is has achieved threshold tippers.
	type TipCountdown: Get<Self::BlockNumber>;

	/// The percent of the final tip which goes to the original reporter of the tip.
	type TipFindersFee: Get<Percent>;

	/// The amount held on deposit for placing a tip report.
	type TipReportDepositBase: Get<BalanceOf<Self>>;

	/// The amount held on deposit per byte within the tip report reason or bounty description.
	type DataDepositPerByte: Get<BalanceOf<Self>>;

	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

	/// Handler for the unbalanced decrease when slashing for a rejected proposal.
	type ProposalRejection: OnUnbalanced<NegativeImbalanceOf<Self>>;

	/// Fraction of a proposal's value that should be bonded in order to place the proposal.
	/// An accepted proposal gets these back. A rejected proposal does not.
	type ProposalBond: Get<Permill>;

	/// Minimum amount of funds that should be placed in a deposit for making a proposal.
	type ProposalBondMinimum: Get<BalanceOf<Self>>;

	/// Period between successive spends.
	type SpendPeriod: Get<Self::BlockNumber>;

	/// Percentage of spare funds (if any) that are burnt per spend period.
	type Burn: Get<Permill>;

	/// The amount held on deposit for placing a bounty proposal.
	type BountyDepositBase: Get<BalanceOf<Self>>;

	/// The delay period for which a bounty beneficiary need to wait before claim the payout.
	type BountyDepositPayoutDelay: Get<Self::BlockNumber>;

	/// Bounty duration in blocks.
	type BountyDuration: Get<Self::BlockNumber>;

	/// Maximum acceptable reason length.
	type MaximumReasonLength: Get<u32>;

	/// Maximum number of sub-bounty creation recursion limit. Cannot be 255 to prevent overflow.
	/// e.g. 0 means no sub-bounty, 1 means sub-bounty cannot create sub-bounty.
	type MaximumSubBountyDepth: Get<u8>;

	/// Handler for the unbalanced decrease when treasury funds are burned.
	type BurnDestination: OnUnbalanced<NegativeImbalanceOf<Self>>;

	/// Weight information for extrinsics in this pallet.
	type WeightInfo: WeightInfo;
}

/// An index of a proposal. Just a `u32`.
pub type ProposalIndex = u32;

/// A spending proposal.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Proposal<AccountId, Balance> {
	/// The account proposing it.
	proposer: AccountId,
	/// The (total) amount that should be paid if the proposal is accepted.
	value: Balance,
	/// The account to whom the payment should be made if the proposal is accepted.
	beneficiary: AccountId,
	/// The amount held on deposit (reserved) for making this proposal.
	bond: Balance,
}

/// An open tipping "motion". Retains all details of a tip including information on the finder
/// and the members who have voted.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug)]
pub struct OpenTip<
	AccountId: Parameter,
	Balance: Parameter,
	BlockNumber: Parameter,
	Hash: Parameter,
> {
	/// The hash of the reason for the tip. The reason should be a human-readable UTF-8 encoded string. A URL would be
	/// sensible.
	reason: Hash,
	/// The account to be tipped.
	who: AccountId,
	/// The account who began this tip.
	finder: AccountId,
	/// The amount held on deposit for this tip.
	deposit: Balance,
	/// The block number at which this tip will close if `Some`. If `None`, then no closing is
	/// scheduled.
	closes: Option<BlockNumber>,
	/// The members who have voted for this tip. Sorted by AccountId.
	tips: Vec<(AccountId, Balance)>,
	/// Whether this tip should result in the finder taking a fee.
	finders_fee: bool,
}

/// An index of a bounty. Just a `u32`.
pub type BountyIndex = u32;

/// A bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Bounty<AccountId, Balance, BlockNumber> {
	/// The account proposing it.
	proposer: AccountId,
	/// The account manages this bounty.
	curator: AccountId,
	/// The (total) amount that should be paid if the bounty is rewarded.
	value: Balance,
	/// The curator fee. Included in value.
	fee: Balance,
	/// The amount held on deposit (reserved) for making this proposal.
	bond: Balance,
	/// The status of this bounty.
	status: BountyStatus<AccountId, BlockNumber>,
	/// The parent bounty id. None if this is top level bounty.
	parent: Option<BountyIndex>,
}

/// The status of a bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum BountyStatus<AccountId, BlockNumber> {
	/// The bounty is proposed and waiting for approval.
	Proposed,
	/// The bounty is approved and waiting to become active at next spend period.
	Approved,
	/// The bounty is active and waiting to be awarded.
	Active {
		expires: BlockNumber,
	},
	/// The bounty is awarded and waiting to released after a delay.
	PendingPayout {
		/// The beneficiary of the bounty.
		beneficiary: AccountId,
		/// When the bounty can be claimed.
		unlock_at: BlockNumber,
	},
}

impl<AccountId, BlockNumber> BountyStatus<AccountId, BlockNumber> {
	pub fn is_active(&self) -> bool {
		match self {
			BountyStatus::Active { .. } => true,
			_ => false,
		}
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as Treasury {
		/// Number of proposals that have been made.
		ProposalCount get(fn proposal_count): ProposalIndex;

		/// Proposals that have been made.
		Proposals get(fn proposals):
			map hasher(twox_64_concat) ProposalIndex
			=> Option<Proposal<T::AccountId, BalanceOf<T>>>;

		/// Proposal indices that have been approved but not yet awarded.
		Approvals get(fn approvals): Vec<ProposalIndex>;

		/// Tips that are not yet completed. Keyed by the hash of `(reason, who)` from the value.
		/// This has the insecure enumerable hash function since the key itself is already
		/// guaranteed to be a secure hash.
		pub Tips get(fn tips):
			map hasher(twox_64_concat) T::Hash
			=> Option<OpenTip<T::AccountId, BalanceOf<T>, T::BlockNumber, T::Hash>>;

		/// Simple preimage lookup from the reason's hash to the original data. Again, has an
		/// insecure enumerable hash since the key is guaranteed to be the result of a secure hash.
		pub Reasons get(fn reasons): map hasher(identity) T::Hash => Option<Vec<u8>>;

		/// Number of bounty proposals that have been made.
		pub BountyCount get(fn bounty_count): BountyIndex;

		/// Bounties that have been made.
		pub Bounties get(fn bounties):
			map hasher(twox_64_concat) BountyIndex
			=> Option<Bounty<T::AccountId, BalanceOf<T>, T::BlockNumber>>;

		/// The description of each bounty.
		pub BountyDescriptions get(fn bounty_descriptions): map hasher(twox_64_concat) BountyIndex => Option<Vec<u8>>;

		/// Bounty indices that have been approved but not yet funded.
		pub BountyApprovals get(fn bounty_approvals): Vec<BountyIndex>;

		/// Minimum value for a bounty or sub-bounty.
		pub BountyValueMinimum get(fn bounty_value_minimum): BalanceOf<T>;
	}
	add_extra_genesis {
		build(|_config| {
			// Create Treasury account
			let _ = T::Currency::make_free_balance_be(
				&<Module<T>>::account_id(),
				T::Currency::minimum_balance(),
			);
		});
	}
}

decl_event!(
	pub enum Event<T>
	where
		Balance = BalanceOf<T>,
		<T as frame_system::Trait>::AccountId,
		<T as frame_system::Trait>::Hash,
	{
		/// New proposal. [proposal_index]
		Proposed(ProposalIndex),
		/// We have ended a spend period and will now allocate funds. [budget_remaining]
		Spending(Balance),
		/// Some funds have been allocated. [proposal_index, award, beneficiary]
		Awarded(ProposalIndex, Balance, AccountId),
		/// A proposal was rejected; funds were slashed. [proposal_index, slashed]
		Rejected(ProposalIndex, Balance),
		/// Some of our funds have been burnt. [burn]
		Burnt(Balance),
		/// Spending has finished; this is the amount that rolls over until next spend. [budget_remaining]
		Rollover(Balance),
		/// Some funds have been deposited. [deposit]
		Deposit(Balance),
		/// A new tip suggestion has been opened. [tip_hash]
		NewTip(Hash),
		/// A tip suggestion has reached threshold and is closing. [tip_hash]
		TipClosing(Hash),
		/// A tip suggestion has been closed. [tip_hash, who, payout]
		TipClosed(Hash, AccountId, Balance),
		/// A tip suggestion has been retracted. [tip_hash]
		TipRetracted(Hash),
		/// New bounty proposal.
		BountyProposed(BountyIndex),
		/// A bounty proposal was rejected; funds were slashed.
		BountyRejected(BountyIndex, Balance),
		/// A bounty proposal is funded and became active.
		BountyBecameActive(BountyIndex),
		/// A bounty is awarded to a beneficiary.
		BountyAwarded(BountyIndex, AccountId),
		/// A bounty is claimed by beneficiary.
		BountyClaimed(BountyIndex, Balance, AccountId),
		/// A bounty is cancelled.
		BountyCanceled(BountyIndex),
		/// A bounty expiry is extended.
		BountyExtended(BountyIndex),
	}
);

decl_error! {
	/// Error for the treasury module.
	pub enum Error for Module<T: Trait> {
		/// Proposer's balance is too low.
		InsufficientProposersBalance,
		/// No proposal or bounty at that index.
		InvalidIndex,
		/// The reason given is just too big.
		ReasonTooBig,
		/// The tip was already found/started.
		AlreadyKnown,
		/// The tip hash is unknown.
		UnknownTip,
		/// The account attempting to retract the tip is not the finder of the tip.
		NotFinder,
		/// The tip cannot be claimed/closed because there are not enough tippers yet.
		StillOpen,
		/// The tip cannot be claimed/closed because it's still in the countdown period.
		Premature,
		/// The bounty status is unexpected.
		UnexpectedStatus,
		/// Require bounty curator.
		RequireCurator,
		/// Invalid bounty value.
		InvalidValue,
		/// Invalid bounty fee.
		InvalidFee,
		/// Sub-bounty cannot be created due to MaximumSubBountyDepth limit.
		ExceedDepthLimit,
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Fraction of a proposal's value that should be bonded in order to place the proposal.
		/// An accepted proposal gets these back. A rejected proposal does not.
		const ProposalBond: Permill = T::ProposalBond::get();

		/// Minimum amount of funds that should be placed in a deposit for making a proposal.
		const ProposalBondMinimum: BalanceOf<T> = T::ProposalBondMinimum::get();

		/// Period between successive spends.
		const SpendPeriod: T::BlockNumber = T::SpendPeriod::get();

		/// Percentage of spare funds (if any) that are burnt per spend period.
		const Burn: Permill = T::Burn::get();

		/// The period for which a tip remains open after is has achieved threshold tippers.
		const TipCountdown: T::BlockNumber = T::TipCountdown::get();

		/// The amount of the final tip which goes to the original reporter of the tip.
		const TipFindersFee: Percent = T::TipFindersFee::get();

		/// The amount held on deposit for placing a tip report.
		const TipReportDepositBase: BalanceOf<T> = T::TipReportDepositBase::get();

		/// The amount held on deposit per byte within the tip report reason or bounty description.
		const DataDepositPerByte: BalanceOf<T> = T::DataDepositPerByte::get();

		/// The treasury's module id, used for deriving its sovereign account ID.
		const ModuleId: ModuleId = T::ModuleId::get();

		/// The amount held on deposit for placing a bounty proposal.
		const BountyDepositBase: BalanceOf<T> = T::BountyDepositBase::get();

		/// The delay period for which a bounty beneficiary need to wait before claim the payout.
		const BountyDepositPayoutDelay: T::BlockNumber = T::BountyDepositPayoutDelay::get();

		/// Maximum acceptable reason length.
		const MaximumReasonLength: u32 = T::MaximumReasonLength::get();

		/// Maximum number of sub-bounty creation recursion limit. Cannot be 255 to prevent overflow.
		/// e.g. 0 means no sub-bounty, 1 means sub-bounty cannot create sub-bounty.
		const MaximumSubBountyDepth: u8 = T::MaximumSubBountyDepth::get();

		type Error = Error<T>;

		fn deposit_event() = default;

		/// Put forward a suggestion for spending. A deposit proportional to the value
		/// is reserved and slashed if the proposal is rejected. It is returned once the
		/// proposal is awarded.
		///
		/// # <weight>
		/// - Complexity: O(1)
		/// - DbReads: `ProposalCount`, `origin account`
		/// - DbWrites: `ProposalCount`, `Proposals`, `origin account`
		/// # </weight>
		#[weight = T::WeightInfo::propose_spend()]
		fn propose_spend(
			origin,
			#[compact] value: BalanceOf<T>,
			beneficiary: <T::Lookup as StaticLookup>::Source
		) {
			let proposer = ensure_signed(origin)?;
			let beneficiary = T::Lookup::lookup(beneficiary)?;

			let bond = Self::calculate_bond(value);
			T::Currency::reserve(&proposer, bond)
				.map_err(|_| Error::<T>::InsufficientProposersBalance)?;

			let c = Self::proposal_count();
			ProposalCount::put(c + 1);
			<Proposals<T>>::insert(c, Proposal { proposer, value, beneficiary, bond });

			Self::deposit_event(RawEvent::Proposed(c));
		}

		/// Reject a proposed spend. The original deposit will be slashed.
		///
		/// May only be called from `T::RejectOrigin`.
		///
		/// # <weight>
		/// - Complexity: O(1)
		/// - DbReads: `Proposals`, `rejected proposer account`
		/// - DbWrites: `Proposals`, `rejected proposer account`
		/// # </weight>
		#[weight = (T::WeightInfo::reject_proposal(), DispatchClass::Operational)]
		fn reject_proposal(origin, #[compact] proposal_id: ProposalIndex) {
			T::RejectOrigin::ensure_origin(origin)?;

			let proposal = <Proposals<T>>::take(&proposal_id).ok_or(Error::<T>::InvalidIndex)?;
			let value = proposal.bond;
			let imbalance = T::Currency::slash_reserved(&proposal.proposer, value).0;
			T::ProposalRejection::on_unbalanced(imbalance);

			Self::deposit_event(Event::<T>::Rejected(proposal_id, value));
		}

		/// Approve a proposal. At a later time, the proposal will be allocated to the beneficiary
		/// and the original deposit will be returned.
		///
		/// May only be called from `T::ApproveOrigin`.
		///
		/// # <weight>
		/// - Complexity: O(1).
		/// - DbReads: `Proposals`, `Approvals`
		/// - DbWrite: `Approvals`
		/// # </weight>
		#[weight = (T::WeightInfo::approve_proposal(), DispatchClass::Operational)]
		fn approve_proposal(origin, #[compact] proposal_id: ProposalIndex) {
			T::ApproveOrigin::ensure_origin(origin)?;

			ensure!(<Proposals<T>>::contains_key(proposal_id), Error::<T>::InvalidIndex);
			Approvals::mutate(|v| v.push(proposal_id));
		}

		/// Report something `reason` that deserves a tip and claim any eventual the finder's fee.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// Payment: `TipReportDepositBase` will be reserved from the origin account, as well as
		/// `DataDepositPerByte` for each byte in `reason`.
		///
		/// - `reason`: The reason for, or the thing that deserves, the tip; generally this will be
		///   a UTF-8-encoded URL.
		/// - `who`: The account which should be credited for the tip.
		///
		/// Emits `NewTip` if successful.
		///
		/// # <weight>
		/// - Complexity: `O(R)` where `R` length of `reason`.
		///   - encoding and hashing of 'reason'
		/// - DbReads: `Reasons`, `Tips`
		/// - DbWrites: `Reasons`, `Tips`
		/// # </weight>
		#[weight = T::WeightInfo::report_awesome(reason.len() as u32)]
		fn report_awesome(origin, reason: Vec<u8>, who: T::AccountId) {
			let finder = ensure_signed(origin)?;

			ensure!(reason.len() <= T::MaximumReasonLength::get() as usize, Error::<T>::ReasonTooBig);

			let reason_hash = T::Hashing::hash(&reason[..]);
			ensure!(!Reasons::<T>::contains_key(&reason_hash), Error::<T>::AlreadyKnown);
			let hash = T::Hashing::hash_of(&(&reason_hash, &who));
			ensure!(!Tips::<T>::contains_key(&hash), Error::<T>::AlreadyKnown);

			let deposit = T::TipReportDepositBase::get()
				+ T::DataDepositPerByte::get() * (reason.len() as u32).into();
			T::Currency::reserve(&finder, deposit)?;

			Reasons::<T>::insert(&reason_hash, &reason);
			let tip = OpenTip {
				reason: reason_hash,
				who,
				finder,
				deposit,
				closes: None,
				tips: vec![],
				finders_fee: true
			};
			Tips::<T>::insert(&hash, tip);
			Self::deposit_event(RawEvent::NewTip(hash));
		}

		/// Retract a prior tip-report from `report_awesome`, and cancel the process of tipping.
		///
		/// If successful, the original deposit will be unreserved.
		///
		/// The dispatch origin for this call must be _Signed_ and the tip identified by `hash`
		/// must have been reported by the signing account through `report_awesome` (and not
		/// through `tip_new`).
		///
		/// - `hash`: The identity of the open tip for which a tip value is declared. This is formed
		///   as the hash of the tuple of the original tip `reason` and the beneficiary account ID.
		///
		/// Emits `TipRetracted` if successful.
		///
		/// # <weight>
		/// - Complexity: `O(1)`
		///   - Depends on the length of `T::Hash` which is fixed.
		/// - DbReads: `Tips`, `origin account`
		/// - DbWrites: `Reasons`, `Tips`, `origin account`
		/// # </weight>
		#[weight = T::WeightInfo::retract_tip(0)]
		fn retract_tip(origin, hash: T::Hash) {
			let who = ensure_signed(origin)?;
			let tip = Tips::<T>::get(&hash).ok_or(Error::<T>::UnknownTip)?;
			ensure!(tip.finder == who, Error::<T>::NotFinder);

			Reasons::<T>::remove(&tip.reason);
			Tips::<T>::remove(&hash);
			if !tip.deposit.is_zero() {
				let _ = T::Currency::unreserve(&who, tip.deposit);
			}
			Self::deposit_event(RawEvent::TipRetracted(hash));
		}

		/// Give a tip for something new; no finder's fee will be taken.
		///
		/// The dispatch origin for this call must be _Signed_ and the signing account must be a
		/// member of the `Tippers` set.
		///
		/// - `reason`: The reason for, or the thing that deserves, the tip; generally this will be
		///   a UTF-8-encoded URL.
		/// - `who`: The account which should be credited for the tip.
		/// - `tip_value`: The amount of tip that the sender would like to give. The median tip
		///   value of active tippers will be given to the `who`.
		///
		/// Emits `NewTip` if successful.
		///
		/// # <weight>
		/// - Complexity: `O(R + T)` where `R` length of `reason`, `T` is the number of tippers.
		///   - `O(T)`: decoding `Tipper` vec of length `T`
		///     `T` is charged as upper bound given by `ContainsLengthBound`.
		///     The actual cost depends on the implementation of `T::Tippers`.
		///   - `O(R)`: hashing and encoding of reason of length `R`
		/// - DbReads: `Tippers`, `Reasons`
		/// - DbWrites: `Reasons`, `Tips`
		/// # </weight>
		#[weight = T::WeightInfo::tip_new(reason.len() as u32, T::Tippers::max_len() as u32)]
		fn tip_new(origin, reason: Vec<u8>, who: T::AccountId, #[compact] tip_value: BalanceOf<T>) {
			let tipper = ensure_signed(origin)?;
			ensure!(T::Tippers::contains(&tipper), BadOrigin);
			let reason_hash = T::Hashing::hash(&reason[..]);
			ensure!(!Reasons::<T>::contains_key(&reason_hash), Error::<T>::AlreadyKnown);
			let hash = T::Hashing::hash_of(&(&reason_hash, &who));

			Reasons::<T>::insert(&reason_hash, &reason);
			Self::deposit_event(RawEvent::NewTip(hash.clone()));
			let tips = vec![(tipper.clone(), tip_value)];
			let tip = OpenTip {
				reason: reason_hash,
				who,
				finder: tipper,
				deposit: Zero::zero(),
				closes: None,
				tips,
				finders_fee: false,
			};
			Tips::<T>::insert(&hash, tip);
		}

		/// Declare a tip value for an already-open tip.
		///
		/// The dispatch origin for this call must be _Signed_ and the signing account must be a
		/// member of the `Tippers` set.
		///
		/// - `hash`: The identity of the open tip for which a tip value is declared. This is formed
		///   as the hash of the tuple of the hash of the original tip `reason` and the beneficiary
		///   account ID.
		/// - `tip_value`: The amount of tip that the sender would like to give. The median tip
		///   value of active tippers will be given to the `who`.
		///
		/// Emits `TipClosing` if the threshold of tippers has been reached and the countdown period
		/// has started.
		///
		/// # <weight>
		/// - Complexity: `O(T)` where `T` is the number of tippers.
		///   decoding `Tipper` vec of length `T`, insert tip and check closing,
		///   `T` is charged as upper bound given by `ContainsLengthBound`.
		///   The actual cost depends on the implementation of `T::Tippers`.
		///
		///   Actually weight could be lower as it depends on how many tips are in `OpenTip` but it
		///   is weighted as if almost full i.e of length `T-1`.
		/// - DbReads: `Tippers`, `Tips`
		/// - DbWrites: `Tips`
		/// # </weight>
		#[weight = T::WeightInfo::tip(T::Tippers::max_len() as u32)]
		fn tip(origin, hash: T::Hash, #[compact] tip_value: BalanceOf<T>) {
			let tipper = ensure_signed(origin)?;
			ensure!(T::Tippers::contains(&tipper), BadOrigin);

			let mut tip = Tips::<T>::get(hash).ok_or(Error::<T>::UnknownTip)?;
			if Self::insert_tip_and_check_closing(&mut tip, tipper, tip_value) {
				Self::deposit_event(RawEvent::TipClosing(hash.clone()));
			}
			Tips::<T>::insert(&hash, tip);
		}

		/// Close and payout a tip.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// The tip identified by `hash` must have finished its countdown period.
		///
		/// - `hash`: The identity of the open tip for which a tip value is declared. This is formed
		///   as the hash of the tuple of the original tip `reason` and the beneficiary account ID.
		///
		/// # <weight>
		/// - Complexity: `O(T)` where `T` is the number of tippers.
		///   decoding `Tipper` vec of length `T`.
		///   `T` is charged as upper bound given by `ContainsLengthBound`.
		///   The actual cost depends on the implementation of `T::Tippers`.
		/// - DbReads: `Tips`, `Tippers`, `tip finder`
		/// - DbWrites: `Reasons`, `Tips`, `Tippers`, `tip finder`
		/// # </weight>
		#[weight = T::WeightInfo::close_tip(T::Tippers::max_len() as u32)]
		fn close_tip(origin, hash: T::Hash) {
			ensure_signed(origin)?;

			let tip = Tips::<T>::get(hash).ok_or(Error::<T>::UnknownTip)?;
			let n = tip.closes.as_ref().ok_or(Error::<T>::StillOpen)?;
			ensure!(system::Module::<T>::block_number() >= *n, Error::<T>::Premature);
			// closed.
			Reasons::<T>::remove(&tip.reason);
			Tips::<T>::remove(hash);
			Self::payout_tip(hash, tip);
		}

		#[weight = T::WeightInfo::propose_bounty(description.len() as u32)]
		fn propose_bounty(
			origin,
			curator: <T::Lookup as StaticLookup>::Source,
			#[compact] fee: BalanceOf<T>,
			#[compact] value: BalanceOf<T>,
			description: Vec<u8>,
		) {
			let proposer = ensure_signed(origin)?;
			let curator = T::Lookup::lookup(curator)?;

			Self::create_bounty(proposer, curator, description, fee, value, None)?;
		}

		#[weight = T::WeightInfo::create_sub_bounty(description.len() as u32)]
		fn create_sub_bounty(
			origin,
			#[compact] parent_bounty_id: BountyIndex,
			curator: <T::Lookup as StaticLookup>::Source,
			#[compact] fee: BalanceOf<T>,
			#[compact] value: BalanceOf<T>,
			description: Vec<u8>,
		) {
			let proposer = ensure_signed(origin)?;
			let curator = T::Lookup::lookup(curator)?;

			Self::create_bounty(proposer, curator, description, fee, value, Some(parent_bounty_id))?;
		}

		/// Reject a bounty proposal. The original deposit will be slashed.
		///
		/// # <weight>
		/// - O(1).
		/// - Limited storage reads.
		/// - Two DB clear.
		/// # </weight>
		#[weight = T::WeightInfo::reject_bounty()]
		fn reject_bounty(origin, #[compact] bounty_id: BountyIndex) {
			T::RejectOrigin::ensure_origin(origin)?;

			Bounties::<T>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_ref().ok_or(Error::<T>::InvalidIndex)?;
				ensure!(bounty.status == BountyStatus::Proposed, Error::<T>::UnexpectedStatus);

				BountyDescriptions::remove(bounty_id);

				let value = bounty.bond;
				let imbalance = T::Currency::slash_reserved(&bounty.proposer, value).0;
				T::ProposalRejection::on_unbalanced(imbalance);

				Self::deposit_event(Event::<T>::BountyRejected(bounty_id, value));

				*maybe_bounty = None;

				Ok(())
			})?;
		}

		/// Approve a bounty proposal. At a later time, the bounty will be funded and become active
		/// and the original deposit will be returned.
		///
		/// # <weight>
		/// - O(1).
		/// - Limited storage reads.
		/// - One DB change.
		/// # </weight>
		#[weight = T::WeightInfo::approve_bounty()]
		fn approve_bounty(origin, #[compact] bounty_id: ProposalIndex) {
			T::ApproveOrigin::ensure_origin(origin)?;

			Bounties::<T>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let mut bounty = maybe_bounty.as_mut().ok_or(Error::<T>::InvalidIndex)?;
				ensure!(bounty.status == BountyStatus::Proposed, Error::<T>::UnexpectedStatus);

				bounty.status = BountyStatus::Approved;

				BountyApprovals::mutate(|v| v.push(bounty_id));

				Ok(())
			})?;
		}

		#[weight = T::WeightInfo::award_bounty()]
		fn award_bounty(origin, #[compact] bounty_id: ProposalIndex, beneficiary: <T::Lookup as StaticLookup>::Source) {
			let curator = ensure_signed(origin)?;
			let beneficiary = T::Lookup::lookup(beneficiary)?;

			Bounties::<T>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let mut bounty = maybe_bounty.as_mut().ok_or(Error::<T>::InvalidIndex)?;
				ensure!(bounty.status.is_active(), Error::<T>::UnexpectedStatus);
				ensure!(bounty.curator == curator, Error::<T>::RequireCurator);
				bounty.status = BountyStatus::PendingPayout {
					beneficiary: beneficiary.clone(),
					unlock_at: system::Module::<T>::block_number() + T::BountyDepositPayoutDelay::get(),
				};

				Ok(())
			})?;

			Self::deposit_event(Event::<T>::BountyAwarded(bounty_id, beneficiary));
		}

		#[weight = T::WeightInfo::claim_bounty()]
		fn claim_bounty(origin, #[compact] bounty_id: BountyIndex) {
			let _ = ensure_signed(origin)?; // anyone can trigger claim

			Bounties::<T>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.take().ok_or(Error::<T>::InvalidIndex)?;
				if let BountyStatus::PendingPayout { beneficiary, unlock_at } = bounty.status {
					ensure!(system::Module::<T>::block_number() >= unlock_at, Error::<T>::Premature);
					let bounty_account = Self::bounty_account_id(bounty_id);
					let balance = T::Currency::free_balance(&bounty_account);
					let fee = bounty.fee;
					let payout = balance.saturating_sub(fee);
					let _ = T::Currency::transfer(&bounty_account, &bounty.curator, fee, AllowDeath); // should not fail
					let _ = T::Currency::transfer(&bounty_account, &beneficiary, payout, AllowDeath); // should not fail
					*maybe_bounty = None;

					BountyDescriptions::remove(bounty_id);

					Self::deposit_event(Event::<T>::BountyClaimed(bounty_id, payout, beneficiary));
					Ok(())
				} else {
					Err(Error::<T>::UnexpectedStatus.into())
				}
			})?;
		}

		#[weight = T::WeightInfo::cancel_bounty()]
		fn cancel_bounty(origin, #[compact] bounty_id: BountyIndex) {
			let curator = ensure_signed(origin)?;

			Bounties::<T>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_ref().ok_or(Error::<T>::InvalidIndex)?;

				match bounty.status {
					BountyStatus::Active { expires } => {
						let now = system::Module::<T>::block_number();
						if expires > now {
							// only curator can cancel unexpired bounty
							ensure!(bounty.curator == curator, Error::<T>::RequireCurator);
						}
					},
					_ => return Err(Error::<T>::UnexpectedStatus.into()),
				}

				let bounty_account = Self::bounty_account_id(bounty_id);

				BountyDescriptions::remove(bounty_id);

				let balance = T::Currency::free_balance(&bounty_account);
				let _ = T::Currency::transfer(&bounty_account, &Self::account_id(), balance, AllowDeath); // should not fail
				*maybe_bounty = None;

				Ok(())
			})?;

			Self::deposit_event(Event::<T>::BountyCanceled(bounty_id));
		}

		#[weight = T::WeightInfo::extend_bounty_expiry()]
		fn extend_bounty_expiry(origin, #[compact] bounty_id: BountyIndex) {
			let curator = ensure_signed(origin)?;

			Bounties::<T>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let mut bounty = maybe_bounty.as_mut().ok_or(Error::<T>::InvalidIndex)?;

				ensure!(bounty.status.is_active(), Error::<T>::UnexpectedStatus);
				ensure!(bounty.curator == curator, Error::<T>::RequireCurator);

				match bounty.status {
					BountyStatus::Active { expires } => {
						let expires = expires.max(system::Module::<T>::block_number() + T::BountyDuration::get());
						bounty.status = BountyStatus::Active { expires };
					},
					_ => return Err(Error::<T>::UnexpectedStatus.into()),
				}

				Ok(())
			})?;

			Self::deposit_event(Event::<T>::BountyExtended(bounty_id));
		}

		#[weight = T::WeightInfo::update_bounty_value_minimum()]
		fn update_bounty_value_minimum(origin, #[compact] new_value: BalanceOf<T>) {
			T::ApproveOrigin::ensure_origin(origin)?;

			BountyValueMinimum::<T>::put(new_value);
		}

		/// # <weight>
		/// - Complexity: `O(A)` where `A` is the number of approvals
		/// - Db reads and writes: `Approvals`, `pot account data`
		/// - Db reads and writes per approval:
		///   `Proposals`, `proposer account data`, `beneficiary account data`
		/// - The weight is overestimated if some approvals got missed.
		/// # </weight>
		fn on_initialize(n: T::BlockNumber) -> Weight {
			// Check to see if we should spend some funds!
			if (n % T::SpendPeriod::get()).is_zero() {
				Self::spend_funds()
			} else {
				0
			}
		}
	}
}

impl<T: Trait> Module<T> {
	// Add public immutables and private mutables.

	/// The account ID of the treasury pot.
	///
	/// This actually does computation. If you need to keep using it, then make sure you cache the
	/// value and only call this once.
	pub fn account_id() -> T::AccountId {
		T::ModuleId::get().into_account()
	}

	/// The account ID of a bounty account
	pub fn bounty_account_id(id: BountyIndex) -> T::AccountId {
		// only use two byte prefix to support 16 byte account id (used by test)
		// "modl" ++ "py/trsry" ++ "bt" is 14 bytes, and two bytes remaining for bounty index
		T::ModuleId::get().into_sub_account(("bt", id))
	}

	/// The needed bond for a proposal whose spend is `value`.
	fn calculate_bond(value: BalanceOf<T>) -> BalanceOf<T> {
		T::ProposalBondMinimum::get().max(T::ProposalBond::get() * value)
	}

	/// Given a mutable reference to an `OpenTip`, insert the tip into it and check whether it
	/// closes, if so, then deposit the relevant event and set closing accordingly.
	///
	/// `O(T)` and one storage access.
	fn insert_tip_and_check_closing(
		tip: &mut OpenTip<T::AccountId, BalanceOf<T>, T::BlockNumber, T::Hash>,
		tipper: T::AccountId,
		tip_value: BalanceOf<T>,
	) -> bool {
		match tip.tips.binary_search_by_key(&&tipper, |x| &x.0) {
			Ok(pos) => tip.tips[pos] = (tipper, tip_value),
			Err(pos) => tip.tips.insert(pos, (tipper, tip_value)),
		}
		Self::retain_active_tips(&mut tip.tips);
		let threshold = (T::Tippers::count() + 1) / 2;
		if tip.tips.len() >= threshold && tip.closes.is_none() {
			tip.closes = Some(system::Module::<T>::block_number() + T::TipCountdown::get());
			true
		} else {
			false
		}
	}

	/// Remove any non-members of `Tippers` from a `tips` vector. `O(T)`.
	fn retain_active_tips(tips: &mut Vec<(T::AccountId, BalanceOf<T>)>) {
		let members = T::Tippers::sorted_members();
		let mut members_iter = members.iter();
		let mut member = members_iter.next();
		tips.retain(|(ref a, _)| loop {
			match member {
				None => break false,
				Some(m) if m > a => break false,
				Some(m) => {
					member = members_iter.next();
					if m < a {
						continue
					} else {
						break true;
					}
				}
			}
		});
	}

	/// Execute the payout of a tip.
	///
	/// Up to three balance operations.
	/// Plus `O(T)` (`T` is Tippers length).
	fn payout_tip(hash: T::Hash, tip: OpenTip<T::AccountId, BalanceOf<T>, T::BlockNumber, T::Hash>) {
		let mut tips = tip.tips;
		Self::retain_active_tips(&mut tips);
		tips.sort_by_key(|i| i.1);
		let treasury = Self::account_id();
		let max_payout = Self::pot();
		let mut payout = tips[tips.len() / 2].1.min(max_payout);
		if !tip.deposit.is_zero() {
			let _ = T::Currency::unreserve(&tip.finder, tip.deposit);
		}
		if tip.finders_fee {
			if tip.finder != tip.who {
				// pay out the finder's fee.
				let finders_fee = T::TipFindersFee::get() * payout;
				payout -= finders_fee;
				// this should go through given we checked it's at most the free balance, but still
				// we only make a best-effort.
				let _ = T::Currency::transfer(&treasury, &tip.finder, finders_fee, KeepAlive);
			}
		}
		// same as above: best-effort only.
		let _ = T::Currency::transfer(&treasury, &tip.who, payout, KeepAlive);
		Self::deposit_event(RawEvent::TipClosed(hash, tip.who, payout));
	}

	/// Spend some money! returns number of approvals before spend.
	fn spend_funds() -> Weight {
		let mut total_weight: Weight = Zero::zero();

		let mut budget_remaining = Self::pot();
		Self::deposit_event(RawEvent::Spending(budget_remaining));
		let account_id = Self::account_id();

		let mut missed_any = false;
		let mut imbalance = <PositiveImbalanceOf<T>>::zero();
		let proposals_len = Approvals::mutate(|v| {
			let proposals_approvals_len = v.len() as u32;
			v.retain(|&index| {
				// Should always be true, but shouldn't panic if false or we're screwed.
				if let Some(p) = Self::proposals(index) {
					if p.value <= budget_remaining {
						budget_remaining -= p.value;
						<Proposals<T>>::remove(index);

						// return their deposit.
						let _ = T::Currency::unreserve(&p.proposer, p.bond);

						// provide the allocation.
						imbalance.subsume(T::Currency::deposit_creating(&p.beneficiary, p.value));

						Self::deposit_event(RawEvent::Awarded(index, p.value, p.beneficiary));
						false
					} else {
						missed_any = true;
						true
					}
				} else {
					false
				}
			});
			proposals_approvals_len
		});

		total_weight += T::WeightInfo::on_initialize_proposals(proposals_len);

		let bounties_len = BountyApprovals::mutate(|v| {
			let bounties_approval_len = v.len() as u32;
			v.retain(|&index| {
				Bounties::<T>::mutate(index, |bounty| {
					// Should always be true, but shouldn't panic if false or we're screwed.
					if let Some(bounty) = bounty {
						if bounty.value <= budget_remaining {
							budget_remaining -= bounty.value;

							// we trust bounty duration is configured with a sane value
							let expires = system::Module::<T>::block_number() + T::BountyDuration::get();
							bounty.status = BountyStatus::Active { expires };

							// return their deposit.
							let _ = T::Currency::unreserve(&bounty.proposer, bounty.bond);

							// fund the bounty account
							imbalance.subsume(T::Currency::deposit_creating(&Self::bounty_account_id(index), bounty.value));

							Self::deposit_event(RawEvent::BountyBecameActive(index));
							false
						} else {
							missed_any = true;
							true
						}
					} else {
						false
					}
				})
			});
			bounties_approval_len
		});

		total_weight += T::WeightInfo::on_initialize_proposals(bounties_len);

		if !missed_any {
			// burn some proportion of the remaining budget if we run a surplus.
			let burn = (T::Burn::get() * budget_remaining).min(budget_remaining);
			budget_remaining -= burn;

			let (debit, credit) = T::Currency::pair(burn);
			imbalance.subsume(debit);
			T::BurnDestination::on_unbalanced(credit);
			Self::deposit_event(RawEvent::Burnt(burn))
		}

		// Must never be an error, but better to be safe.
		// proof: budget_remaining is account free balance minus ED;
		// Thus we can't spend more than account free balance minus ED;
		// Thus account is kept alive; qed;
		if let Err(problem) = T::Currency::settle(
			&account_id,
			imbalance,
			WithdrawReason::Transfer.into(),
			KeepAlive
		) {
			print("Inconsistent state - couldn't settle imbalance for funds spent by treasury");
			// Nothing else to do here.
			drop(problem);
		}

		Self::deposit_event(RawEvent::Rollover(budget_remaining));

		total_weight
	}

	/// Return the amount of money in the pot.
	// The existential deposit is not part of the pot so treasury account never gets deleted.
	fn pot() -> BalanceOf<T> {
		T::Currency::free_balance(&Self::account_id())
			// Must never be less than 0 but better be safe.
			.saturating_sub(T::Currency::minimum_balance())
	}

	fn check_bounty_depth(parent_bounty_id: Option<BountyIndex>, depth: u8) -> bool {
		if let Some(id) = parent_bounty_id {
			if depth == 0 {
				return false
			}
			Self::check_bounty_depth(Self::bounties(id).and_then(|b| b.parent), depth - 1)
		} else {
			true
		}
	}

	fn create_bounty(
		proposer: T::AccountId,
		curator: T::AccountId,
		description: Vec<u8>,
		fee: BalanceOf<T>,
		value: BalanceOf<T>,
		parent_bounty_id: Option<BountyIndex>,
	) -> DispatchResult {
		ensure!(description.len() <= T::MaximumReasonLength::get() as usize, Error::<T>::ReasonTooBig);
		ensure!(value >= Self::bounty_value_minimum(), Error::<T>::InvalidValue);
		ensure!(fee < value, Error::<T>::InvalidFee);

		let index = Self::bounty_count();

		let (bond, status, is_sub) = if let Some(parent_bounty_id) = parent_bounty_id {
			// this is a sub bounty
			Bounties::<T>::try_mutate_exists(
				parent_bounty_id,
				|bounty| -> DispatchResult {
					let parent = bounty.as_mut().ok_or(Error::<T>::InvalidIndex)?;
					ensure!(parent.status.is_active(), Error::<T>::UnexpectedStatus);
					ensure!(proposer == parent.curator, Error::<T>::RequireCurator);
					ensure!(fee < parent.fee, Error::<T>::InvalidFee);
					ensure!(value < parent.value, Error::<T>::InvalidValue);
					ensure!(
						Self::check_bounty_depth(Some(parent_bounty_id), T::MaximumSubBountyDepth::get()),
						Error::<T>::ExceedDepthLimit
					);

					let bounty_acc = Self::bounty_account_id(index);
					let parent_acc = Self::bounty_account_id(parent_bounty_id);

					// fund sub bounty from parent bounty
					T::Currency::transfer(&parent_acc, &bounty_acc, value, KeepAlive)?;

					// Already checked cannot underflow.
					parent.fee -= fee;
					parent.value -= value;

					Ok(())
				}
			)?;

			// we trust bounty duration is configured with a sane value
			let expires = system::Module::<T>::block_number() + T::BountyDuration::get();
			(0.into(), BountyStatus::Active { expires }, true)
		} else {
			// reserve deposit for new bounty
			let bond = T::BountyDepositBase::get()
				+ T::DataDepositPerByte::get() * (description.len() as u32).into();
			T::Currency::reserve(&proposer, bond)
				.map_err(|_| Error::<T>::InsufficientProposersBalance)?;

			(bond, BountyStatus::Proposed, false)
		};

		BountyCount::put(index + 1);

		let bounty = Bounty {
			proposer, curator, value, fee, bond, status, parent: parent_bounty_id,
		};

		Bounties::<T>::insert(index, &bounty);
		BountyDescriptions::insert(index, description);

		Self::deposit_event(RawEvent::BountyProposed(index));
		if is_sub {
			Self::deposit_event(RawEvent::BountyBecameActive(index));
		}

		Ok(())
	}

	pub fn migrate_retract_tip_for_tip_new() {
		/// An open tipping "motion". Retains all details of a tip including information on the finder
		/// and the members who have voted.
		#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug)]
		pub struct OldOpenTip<
			AccountId: Parameter,
			Balance: Parameter,
			BlockNumber: Parameter,
			Hash: Parameter,
		> {
			/// The hash of the reason for the tip. The reason should be a human-readable UTF-8 encoded string. A URL would be
			/// sensible.
			reason: Hash,
			/// The account to be tipped.
			who: AccountId,
			/// The account who began this tip and the amount held on deposit.
			finder: Option<(AccountId, Balance)>,
			/// The block number at which this tip will close if `Some`. If `None`, then no closing is
			/// scheduled.
			closes: Option<BlockNumber>,
			/// The members who have voted for this tip. Sorted by AccountId.
			tips: Vec<(AccountId, Balance)>,
		}

		use frame_support::{Twox64Concat, migration::StorageKeyIterator};

		for (hash, old_tip) in StorageKeyIterator::<
			T::Hash,
			OldOpenTip<T::AccountId, BalanceOf<T>, T::BlockNumber, T::Hash>,
			Twox64Concat,
		>::new(b"Treasury", b"Tips").drain()
		{
			let (finder, deposit, finders_fee) = match old_tip.finder {
				Some((finder, deposit)) => (finder, deposit, true),
				None => (T::AccountId::default(), Zero::zero(), false),
			};
			let new_tip = OpenTip {
				reason: old_tip.reason,
				who: old_tip.who,
				finder,
				deposit,
				closes: old_tip.closes,
				tips: old_tip.tips,
				finders_fee
			};
			Tips::<T>::insert(hash, new_tip)
		}
	}
}

impl<T: Trait> OnUnbalanced<NegativeImbalanceOf<T>> for Module<T> {
	fn on_nonzero_unbalanced(amount: NegativeImbalanceOf<T>) {
		let numeric_amount = amount.peek();

		// Must resolve into existing but better to be safe.
		let _ = T::Currency::resolve_creating(&Self::account_id(), amount);

		Self::deposit_event(RawEvent::Deposit(numeric_amount));
	}
}
