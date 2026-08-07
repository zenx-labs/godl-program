//! §5.5(b): instruction-data parsing for the recently added instructions must
//! error cleanly on arbitrary bytes, never panic. The handlers receive `data`
//! with the discriminant already stripped by `parse_instruction`, so that is
//! what gets fuzzed here.

#![no_main]

use godl_api::instruction::{
    ClosePhantomStakeV2, CloseStakeV2, GodlInstruction, MergeStakeV2, MigrateStakeWeight,
    RebaseTotalStaked, TopUpStakeV2,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = MigrateStakeWeight::try_from_bytes(data);
    let _ = RebaseTotalStaked::try_from_bytes(data);
    let _ = CloseStakeV2::try_from_bytes(data);
    let _ = ClosePhantomStakeV2::try_from_bytes(data);
    let _ = TopUpStakeV2::try_from_bytes(data);
    let _ = MergeStakeV2::try_from_bytes(data);
    if let Some((&disc, _)) = data.split_first() {
        let _ = GodlInstruction::try_from(disc);
    }
});
