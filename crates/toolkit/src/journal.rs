use alloy_sol_types::sol;
use risc0_steel::Commitment;

// ABI encodable journal data.
sol! {
    struct SpanSequence {
        uint64 height;
        uint32 start;
        uint32 size;
    }

    struct Journal {
        Commitment commitment;
        address blobstreamAddress;
        SpanSequence indexBlob;
    }
}

impl From<crate::SpanSequence> for SpanSequence {
    fn from(span_sequence: crate::SpanSequence) -> Self {
        Self {
            height: span_sequence.height,
            start: span_sequence.start,
            size: span_sequence.size,
        }
    }
}
