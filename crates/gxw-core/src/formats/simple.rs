//! SCHOOL-observed framing profile, not a GX Works2 binary specification.
use crate::{
    FramingStatus, GxwError, OpaqueRegion, ParseDiagnostic, RawProgram, RawToken, ReadLimits,
    SourceSpan,
};

const START: usize = 0x4f;
const TRAILER: usize = 20;
pub(crate) const PROFILE: &str = "simple-le-observed-v1";
mod catalog;
pub(crate) mod decoder;

fn span(source: &SourceSpan, start: usize, end: usize) -> SourceSpan {
    SourceSpan {
        source_id: source.source_id,
        offset: start as u64,
        length: (end - start) as u64,
    }
}

fn problem(program: &mut RawProgram, code: &str, message: &str, at: SourceSpan) {
    program.diagnostics.push(ParseDiagnostic {
        code: code.into(),
        severity: "warning".into(),
        message: message.into(),
        source: Some(at),
    });
}

/// Gates are observed header families; bytes 34..50 are preserved without
/// interpreting their apparent timestamp. Names and CPU literals cannot select this profile.
fn header_matches(data: &[u8]) -> bool {
    data.len() >= START
        && data[..12] == [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0]
        && matches!(
            (&data[12..20], &data[63..67]),
            ([0, 0, 0, 0, 0, 0, 0, 0], [0, 0, 0, 0] | [1, 0, 0, 0])
                | ([1, 0, 10, 0, 0, 0, 0, 0], [1, 0, 0, 0])
        )
        && data[20..34] == [2, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0]
        && data[50..55] == [0, 0, 0, 0, 1]
        && data[67..79] == [12, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255]
}

pub(crate) fn tokenize(
    program: &mut RawProgram,
    data: &[u8],
    limits: &ReadLimits,
    total: &mut u64,
) -> Result<(), GxwError> {
    let source = program.source.clone().expect("resolved program source");
    if !header_matches(data) {
        problem(
            program,
            "GXW_POU_PROFILE_UNSUPPORTED",
            "POU header is outside the observed simple-ladder framing profile",
            source.clone(),
        );
        program.opaque_regions.push(OpaqueRegion {
            source,
            reason: "unsupported_profile".into(),
        });
        return Ok(());
    }
    program.profile = Some(PROFILE.into());
    program.framing_status = FramingStatus::Partial;
    let a = u32::from_le_bytes(data[0x37..0x3b].try_into().unwrap()) as usize;
    let b = u32::from_le_bytes(data[0x3b..0x3f].try_into().unwrap()) as usize;
    let expected = START.checked_add(a);
    // Independently supplied FX2N GXW/CSV pair: same counted region plus
    // four extra zero bytes, only observed with this header family.
    let extra = usize::from(
        data[12..20] == [1, 0, 10, 0, 0, 0, 0, 0]
            && expected.and_then(|n| n.checked_add(4)) == Some(data.len()),
    ) * 4;
    if extra == 4 {
        program.profile = Some("simple-le-observed-v2-trailer24".into());
    }
    let reason = if a != b {
        Some(("GXW_POU_LENGTH_MISMATCH", "duplicated POU lengths disagree"))
    } else if a < TRAILER {
        Some((
            "GXW_POU_LENGTH_INVALID",
            "POU length is shorter than its trailer",
        ))
    } else if expected.and_then(|n| n.checked_add(extra)) != Some(data.len()) {
        Some((
            "GXW_POU_SIZE_MISMATCH",
            "POU length does not match an observed trailer layout",
        ))
    } else {
        None
    };
    if let Some((code, message)) = reason {
        problem(program, code, message, span(&source, 0x37, 0x3f));
        program.opaque_regions.push(OpaqueRegion {
            source,
            reason: "unverified_lengths".into(),
        });
        return Ok(());
    }
    let end = data.len() - TRAILER - extra;
    if data[end..].iter().any(|&byte| byte != 0) {
        problem(
            program,
            "GXW_POU_TRAILER_UNSUPPORTED",
            "POU trailer is not all zero bytes",
            span(&source, end, data.len()),
        );
        program.opaque_regions.push(OpaqueRegion {
            source,
            reason: "unverified_trailer".into(),
        });
        return Ok(());
    }
    program.token_region = Some(span(&source, START, end));
    program.trailer = Some(span(&source, end, data.len()));
    program.opaque_regions.push(OpaqueRegion {
        source: span(&source, 0, START),
        reason: "header_semantics_not_decoded".into(),
    });
    let mut cursor = START;
    while cursor < end {
        let size = data[cursor] as usize;
        if size < 3 || size > end - cursor || data[cursor + size - 1] as usize != size {
            problem(
                program,
                "GXW_TOKEN_FRAME_INVALID",
                "cannot establish the next token boundary; remaining token region is opaque",
                span(&source, cursor, end),
            );
            program.opaque_regions.push(OpaqueRegion {
                source: span(&source, cursor, end),
                reason: "invalid_token_boundary".into(),
            });
            return Ok(());
        }
        *total = total
            .checked_add(1)
            .ok_or_else(|| GxwError::format("POU", "token count overflow"))?;
        limits.check("tokens", *total, limits.max_tokens)?;
        program.tokens.push(RawToken {
            ordinal: program.tokens.len(),
            source: span(&source, cursor, cursor + size),
        });
        cursor += size;
    }
    // This is an observed terminal record, not a decoded END instruction.
    let terminal = program.tokens.last().is_some_and(|token| {
        let start = token.source.offset as usize;
        data[start..end] == [3, 0x34, 3]
    });
    if !terminal {
        problem(
            program,
            "GXW_POU_TERMINATOR_MISSING",
            "token region does not end with the observed 03 34 03 record",
            span(&source, START, end),
        );
        return Ok(());
    }
    program.framing_status = FramingStatus::Complete;
    Ok(())
}
