use arrayvec::ArrayVec;

use super::encodation_type::EncodationType;
use super::{ascii, EncodationError, EncodingContext};

pub(super) const UNLATCH: u8 = 0b011111;

#[inline]
fn is_encodable(ch: u8) -> bool {
    matches!(ch, 32..=94)
}

/// Encode 1 to 4 characters using EDIFACT and write it to the context.
fn write4<T: EncodingContext>(ctx: &mut T, s: &ArrayVec<[u8; 4]>) {
    let s1 = s.get(1).cloned().unwrap_or(0) & 0b11_1111;
    ctx.push((s[0] << 2) | (s1 >> 4));

    if s.len() >= 2 {
        let s2 = s.get(2).cloned().unwrap_or(0) & 0b11_1111;
        ctx.push((s1 << 4) | (s2 >> 2));

        if s.len() >= 3 {
            let s3 = s.get(3).cloned().unwrap_or(0) & 0b11_1111;
            ctx.push((s2 << 6) | s3);
        }
    }
}

fn handle_end<T: EncodingContext>(
    ctx: &mut T,
    mut s: ArrayVec<[u8; 4]>,
) -> Result<(), EncodationError> {
    // check case "encoding with <= 2 ASCII, no UNLATCH"
    let rest_chars = s.len() + ctx.characters_left();
    if 0 < rest_chars && rest_chars <= 4 {
        // The standard allows ASCII encoding without UNLATCH if there
        // are <= 2 words of space left in the symbol and
        // we can encode the rest with ASCII in this space.
        let rest: ArrayVec<[u8; 4]> = s
            .iter()
            .cloned()
            .chain(ctx.rest().iter().cloned())
            .collect();
        let ascii_size = ascii::encoding_size(&rest);
        if ascii_size <= 2 {
            match ctx.symbol_size_left(ascii_size).map(|x| x + ascii_size) {
                Some(space) if space <= 2 && ascii_size <= space => {
                    ctx.backup(s.len());
                    ctx.set_mode(EncodationType::Ascii);
                    return Ok(());
                }
                _ => (),
            }
        }
    }
    if s.is_empty() {
        if !ctx.has_more_characters() {
            // eod
            let space_left = ctx
                .symbol_size_left(0)
                .ok_or(EncodationError::NotEnoughSpace)?
                > 0;
            if space_left {
                // padding case
                ctx.push(UNLATCH << 2);
                ctx.set_mode(EncodationType::Ascii);
            }
        } else {
            // mode switch
            ctx.push(UNLATCH << 2);
        }
    } else {
        // eod, maybe add UNLATCH for padding if space allows
        debug_assert!(s.len() <= 3);
        debug_assert!(!ctx.has_more_characters());
        let space_left = ctx
            .symbol_size_left(s.len())
            .ok_or(EncodationError::NotEnoughSpace)?
            > 0;
        if space_left || s.len() == 3 {
            s.push(UNLATCH);
            ctx.set_mode(EncodationType::Ascii);
        }
        write4(ctx, &s);
    }
    Ok(())
}

pub(super) fn encode<T: EncodingContext>(ctx: &mut T) -> Result<(), EncodationError> {
    let mut symbols = ArrayVec::<[u8; 4]>::new();
    while let Some(ch) = ctx.eat() {
        if !is_encodable(ch) {
            return Err(EncodationError::IllegalEdifactCharacter);
            // otherwise treat this as mode switch, like
            //   ctx.backup(symbols.len() + 1);
            //   ctx.push(UNLATCH);
            //   ctx.set_mode(EncodationType::Ascii);
            //   return Ok(());
            // but this can lead to the encoder getting "stuck" if
            // it switches back to edifact directly, this is probably
            // and look_ahead issue.
        }
        symbols.push(ch);

        if symbols.len() == 4 {
            write4(ctx, &symbols);
            symbols.clear();
            if ctx.maybe_switch_mode() {
                break;
            }
        }
    }
    handle_end(ctx, symbols)
}

#[test]
fn test_write4_four() {
    use super::tests::DummyLogic;
    let mut enc = DummyLogic::new(vec![], 3, -1);
    write4(&mut enc, &[0b10_01_00, 0b11_01_10, 0b011010, 1].into());
    assert_eq!(
        enc.codewords,
        vec![0b10_01_00_11, 0b01_10_01_10, 0b10_00_00_01]
    );
}

#[test]
fn test_write4_three() {
    use super::tests::DummyLogic;
    let mut enc = DummyLogic::new(vec![], 3, -1);
    let mut s = ArrayVec::<[u8; 4]>::new();
    s.try_extend_from_slice(&[0b10_01_00, 0b11_01_10, 0b011010])
        .unwrap();
    write4(&mut enc, &s);
    assert_eq!(
        enc.codewords,
        vec![0b10_01_00_11, 0b01_10_01_10, 0b10_00_00_00]
    );
}

#[test]
fn test_write4_two() {
    use super::tests::DummyLogic;
    let mut enc = DummyLogic::new(vec![], 2, -1);
    let mut s = ArrayVec::<[u8; 4]>::new();
    s.try_extend_from_slice(&[0b10_01_00, 0b11_01_10]).unwrap();
    write4(&mut enc, &s);
    assert_eq!(enc.codewords, vec![0b10_01_00_11, 0b01_10_00_00]);
}

#[test]
fn test_write4_one() {
    use super::tests::DummyLogic;
    let mut enc = DummyLogic::new(vec![], 1, -1);
    let mut s = ArrayVec::<[u8; 4]>::new();
    s.try_extend_from_slice(&[0b10_01_00]).unwrap();
    write4(&mut enc, &s);
    assert_eq!(enc.codewords, vec![0b10_01_00_00]);
}
