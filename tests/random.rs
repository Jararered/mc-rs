//! Seed parsing, pinned to Beta 1.7.3 `GuiCreateWorld.actionPerformed`:
//! `Long.parseLong` when the text is numeric, `String.hashCode()` otherwise.

use game::random::parse_seed;

#[test]
fn numeric_text_is_the_long_itself() {
    assert_eq!(parse_seed("0"), 0);
    assert_eq!(parse_seed("123"), 123);
    // The reference server world's seed is negative; it must keep its bit
    // pattern rather than wrapping into a different magnitude.
    assert_eq!(
        parse_seed("-5779659068535663308"),
        (-5779659068535663308_i64).cast_unsigned()
    );
    assert_eq!(parse_seed("9223372036854775807"), i64::MAX.cast_unsigned());
}

#[test]
fn whitespace_is_not_trimmed_because_the_reference_does_not_trim() {
    // `Long.parseLong` rejects surrounding whitespace, so this falls through to
    // the hash and never becomes the number it looks like.
    assert_eq!(parse_seed("  42  "), 947_283_710);
    assert_ne!(parse_seed("  42  "), 42);
}

#[test]
fn a_negative_text_hash_is_sign_extended_to_sixty_four_bits() {
    // "gargamel".hashCode() is -1_623_774_494. The reference casts that `int` to
    // a `long`, so bits 32 through 47 are set. Zero-extending instead would
    // change the 48-bit value the generator masks down to.
    assert_eq!(parse_seed("gargamel"), (-1_623_774_494_i64).cast_unsigned());
    assert_ne!(
        parse_seed("gargamel"),
        2_671_192_802,
        "a zero-extended 32-bit hash would diverge once masked to 48 bits"
    );
}

#[test]
fn text_seeds_use_java_string_hash() {
    // Reference values computed as `h = h * 31 + c` over the ASCII code units.
    assert_eq!(parse_seed("a"), 97);
    assert_eq!(parse_seed("hello"), 99_162_322);
    assert_eq!(parse_seed("glacier"), 108_181_935);
    assert_eq!(parse_seed(""), 0);
}

#[test]
fn a_text_hash_that_overflows_i32_stays_negative() {
    // 2,869,595,232 as an `i32` is -1,425,372,064, exactly as Java reports.
    let expected = -1_425_372_064_i64;
    assert_eq!(parse_seed("aaaaaa"), expected.cast_unsigned());
    assert!(
        parse_seed("aaaaaa") > i64::MAX.cast_unsigned(),
        "the wrapped seed must keep its bit pattern above i64::MAX"
    );
}

#[test]
fn non_bmp_characters_hash_their_utf16_surrogates() {
    // Java hashes UTF-16 code units, so "𝄞" is the surrogates 0xD834 and
    // 0xDD1E, not the code point 0x1D11E.
    assert_eq!(parse_seed("𝄞"), 1_772_394);
    assert_ne!(parse_seed("𝄞"), 119_070);
}
