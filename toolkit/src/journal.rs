use alloy_sol_types::sol;
use risc0_steel::Commitment;

// ABI encodable journal data.
sol! {
    struct Journal {
        Commitment commitment;
    }
}
