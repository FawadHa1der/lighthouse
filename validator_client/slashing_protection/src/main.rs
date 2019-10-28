extern crate fs2;

use fs2::FileExt;
use parking_lot::Mutex;
use slashing_protection::attester_slashings::{
    check_for_attester_slashing, ValidatorHistoricalAttestation,
};
use slashing_protection::proposer_slashings::{
    check_for_proposer_slashing, ValidatorHistoricalBlock,
};
use ssz::{Decode, Encode};
use std::convert::TryFrom;
use std::fs::File;
use std::io::{Read, Result as IOResult, Write};
use std::sync::Arc;
use std::thread;
use std::time;
use types::*;

const BLOCK_HISTORY_FILE: &str = "block.file";
const ATTESTATION_HISTORY_FILE: &str = "attestation.file";

// enum Safety {
// 	Safe {index: usize, reason: Reason}, // look for error types
// 	NotSafe(Reason)
// }

trait SlashingSafety<T> {
    type U;

    fn is_safe_from_slashings(
        &self,
        challenger: &Self::U,
        history: &[T],
    ) -> Result<usize, &'static str>;
}

impl SlashingSafety<ValidatorHistoricalAttestation>
    for HistoryInfo<ValidatorHistoricalAttestation>
{
    type U = AttestationData;

    fn is_safe_from_slashings(
        &self,
        challenger: &AttestationData,
        history: &[ValidatorHistoricalAttestation],
    ) -> Result<usize, &'static str> {
        check_for_attester_slashing(challenger, history).map_err(|_| "invalid attestation")
    }
}

impl SlashingSafety<ValidatorHistoricalBlock> for HistoryInfo<ValidatorHistoricalBlock> {
    type U = BeaconBlockHeader;

    fn is_safe_from_slashings(
        &self,
        challenger: &BeaconBlockHeader,
        history: &[ValidatorHistoricalBlock],
    ) -> Result<usize, &'static str> {
        check_for_proposer_slashing(challenger, history)
    }
}

#[derive(Debug)]
struct HistoryInfo<T: Encode + Decode + Clone> {
    filename: String,
    mutex: Arc<Mutex<Vec<T>>>,
}

impl<T: Encode + Decode + Clone> HistoryInfo<T> {
    pub fn update_and_write(&mut self) -> IOResult<()> {
        println!("{}: waiting for mutex", self.filename);
        let history = self.mutex.lock(); // SCOTT: check here please
        println!("{}: mutex acquired", self.filename);
        // insert
        let mut file = File::create(self.filename.as_str()).unwrap();
        println!("{}: waiting for file", self.filename);
        file.lock_exclusive()?;
        println!("{}: file acquired", self.filename);
        // go_to_sleep(100); // nope
        file.write_all(&history.as_ssz_bytes()).expect("HEY"); // nope
        file.unlock()?;
        println!("{}: file unlocked", self.filename);

        Ok(())
    }

    fn check_for_slashing(
        &self,
        challenger: &<HistoryInfo<T> as SlashingSafety<T>>::U,
    ) -> Result<usize, &'static str>
    where
        Self: SlashingSafety<T>,
    {
        let guard = self.mutex.lock();
        let history = &guard[..];
        self.is_safe_from_slashings(challenger, history)
    }
}

impl<T: Encode + Decode + Clone> TryFrom<&str> for HistoryInfo<T> {
    type Error = &'static str;

    fn try_from(filename: &str) -> Result<Self, Self::Error> {
        let mut file = File::open(filename).unwrap();
        file.lock_exclusive().unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        file.unlock().unwrap();

        let history = Vec::from_ssz_bytes(&bytes).unwrap();
        let attestation_history = history.to_vec();

        let data_mutex = Mutex::new(attestation_history);
        let arc_data = Arc::new(data_mutex);

        Ok(Self {
            filename: filename.to_string(),
            mutex: arc_data,
        })
    }
}

fn main() {
    run();
}

fn go_to_sleep(time: u64) {
    let ten_millis = time::Duration::from_millis(time);
    thread::sleep(ten_millis);
}

fn run() {
    let mut handles = vec![];

    for _ in 0..4 {
        let handle = thread::spawn(move || {
            let mut attestation_info: HistoryInfo<ValidatorHistoricalAttestation> =
                HistoryInfo::try_from(ATTESTATION_HISTORY_FILE).unwrap();
            let mut block_info: HistoryInfo<ValidatorHistoricalBlock> =
                HistoryInfo::try_from(BLOCK_HISTORY_FILE).unwrap();
            let attestation = attestation_builder(1, 2);
            let block = block_builder(1);
            let res = attestation_info.check_for_slashing(&attestation);
            let res = block_info.check_for_slashing(&block);
            go_to_sleep(1000);
            attestation_info.update_and_write().unwrap();
            block_info.update_and_write().unwrap();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

fn attestation_builder(source: u64, target: u64) -> AttestationData {
    let source = build_checkpoint(source);
    let target = build_checkpoint(target);
    let crosslink = Crosslink::default();

    AttestationData {
        beacon_block_root: Hash256::zero(),
        source,
        target,
        crosslink,
    }
}

fn block_builder(slot: u64) -> BeaconBlockHeader {
    BeaconBlockHeader {
        slot: Slot::from(slot),
        parent_root: Hash256::random(),
        state_root: Hash256::random(),
        body_root: Hash256::random(),
        signature: Signature::empty_signature(),
    }
}

fn build_checkpoint(epoch_num: u64) -> Checkpoint {
    Checkpoint {
        epoch: Epoch::from(epoch_num),
        root: Hash256::zero(),
    }
}
