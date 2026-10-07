//! Comparison of the immutable pre-Trim SVG oracle across host math libraries.
//!
//! Windows job 111428339776 differs from the Linux frame-30 oracle in exactly
//! 15 path-coordinate occurrences, each by one or two f64 ULPs. No other byte
//! differs. Permit only that measured bound in path data; all markup, commands,
//! separators, transforms, paint, gradient, opacity and identifier bytes remain
//! exact. This helper must never be used for same-runtime no-op identity checks.

const MAX_COORDINATE_ULPS: u64 = 2;

fn is_number_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E')
}

fn number(data: &str) -> Result<(&str, f64, &str), String> {
    let end = data
        .bytes()
        .position(|byte| !is_number_byte(byte))
        .unwrap_or(data.len());
    let (text, rest) = data.split_at(end);
    let value = text
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("invalid finite path coordinate {text:?}"))?;
    Ok((text, value, rest))
}

fn compare_path(mut actual: &str, mut expected: &str) -> Result<(), String> {
    let mut coordinate = 0;
    while let Some(byte) = expected.bytes().next() {
        if byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.') {
            let (expected_text, expected_value, expected_rest) = number(expected)?;
            let (actual_text, actual_value, actual_rest) = number(actual)?;
            // Same-sign finite values have monotonically ordered magnitudes in
            // their bit representations. Do not add an absolute-error floor:
            // small coordinates, signed zero and sign changes stay protected.
            let ulps = actual_value.to_bits().abs_diff(expected_value.to_bits());
            if actual_value.is_sign_negative() != expected_value.is_sign_negative()
                || ulps > MAX_COORDINATE_ULPS
            {
                return Err(format!(
                    "coordinate {coordinate}: {actual_text} != {expected_text} ({ulps} ULPs)"
                ));
            }
            actual = actual_rest;
            expected = expected_rest;
            coordinate += 1;
        } else {
            if actual.bytes().next() != Some(byte) {
                return Err(format!(
                    "path command/separator differs before coordinate {coordinate}"
                ));
            }
            actual = &actual[1..];
            expected = &expected[1..];
        }
    }
    if actual.is_empty() {
        Ok(())
    } else {
        Err("extra path data".into())
    }
}

pub(super) fn compare(mut actual: &str, mut expected: &str) -> Result<(), String> {
    let mut path = 0;
    while let Some((expected_head, expected_tail)) = expected.split_once("<path d='") {
        let (actual_head, actual_tail) = actual
            .split_once("<path d='")
            .ok_or_else(|| format!("missing path {path}"))?;
        if actual_head != expected_head {
            return Err(format!("non-path-data bytes differ before path {path}"));
        }
        let (expected_data, expected_rest) = expected_tail
            .split_once('\'')
            .ok_or("unterminated oracle path data")?;
        let (actual_data, actual_rest) = actual_tail
            .split_once('\'')
            .ok_or("unterminated actual path data")?;
        compare_path(actual_data, expected_data)
            .map_err(|error| format!("path {path}: {error}"))?;
        actual = actual_rest;
        expected = expected_rest;
        path += 1;
    }
    if actual == expected {
        Ok(())
    } else {
        Err(format!("non-path-data bytes differ after {path} paths"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_recorded_windows_frame_30_without_replacing_the_oracle() {
        // Full failure log: https://github.com/whitedev7773/LibreEffects/actions/runs/37199589130/job/111428339776
        // These three replacements reconstruct every changed byte in its left
        // operand. The CRC pins the whole observed SVG, not a regenerated oracle.
        let records: serde_json::Value =
            serde_json::from_str(include_str!("legacy_svg.json")).unwrap();
        let expected = records
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["frame"] == 30)
            .unwrap()["svg"]
            .as_str()
            .unwrap();
        let mut actual = expected.to_owned();
        for (linux, windows, occurrences) in [
            ("123.14592810377826", "123.14592810377823", 3),
            ("-46.547933140207995", "-46.54793314020799", 3),
            ("121.28633282414309", "121.28633282414307", 9),
        ] {
            assert_eq!(actual.matches(linux).count(), occurrences);
            actual = actual.replace(linux, windows);
        }
        assert_ne!(actual, expected);
        assert_eq!(actual.len(), 23_399);
        assert_eq!(crc32fast::hash(actual.as_bytes()), 0x3b6d_3b9f);
        compare(&actual, expected).unwrap();
    }

    #[test]
    fn coordinate_roundoff_is_bounded_without_an_absolute_error_floor() {
        for expected in [123.14592810377826_f64, -46.547933140207995, 1e-30, -1e-30] {
            let svg = |value| format!("<path d='M{value} 0'/>");
            for ulps in 0..=MAX_COORDINATE_ULPS {
                let actual = f64::from_bits(expected.to_bits() + ulps);
                compare(&svg(actual), &svg(expected)).unwrap();
            }
            let outside = f64::from_bits(expected.to_bits() + MAX_COORDINATE_ULPS + 1);
            assert!(compare(&svg(outside), &svg(expected)).is_err());
        }
        for actual in [
            "0.00000000000000000001",
            "-1e-30",
            "NaN",
            "inf",
            "1e999",
            "1..0",
        ] {
            assert!(compare(&format!("<path d='M{actual} 0'/>"), "<path d='M1e-30 0'/>").is_err());
        }
        assert!(compare("<path d='M-0 0'/>", "<path d='M0 0'/>").is_err());
    }

    #[test]
    fn commands_structure_and_all_other_attributes_remain_byte_exact() {
        let expected = "<g transform='matrix(1 0 0 1 0 0)' opacity='0.5'><defs><radialGradient id='g123-4' r='1' fx='1' fy='1'><stop offset='0' stop-color='rgb(1%,2%,3%)' stop-opacity='0.5'/></radialGradient></defs><path d='M1 2 C3 4 5 6 7 8 Z' fill='url(#g123-4)' stroke='#010203' stroke-width='1' stroke-opacity='0.5' fill-rule='evenodd'/><path d='M9 10'/></g>";
        compare(expected, expected).unwrap();
        for (from, to) in [
            ("matrix(1", "matrix(1.0000000000000002"),
            ("opacity='0.5'", "opacity='0.5000000000000001'"),
            ("id='g123-4'", "id='g124-4'"),
            ("r='1'", "r='1.0000000000000002'"),
            ("fx='1'", "fx='1.0000000000000002'"),
            ("fy='1'", "fy='1.0000000000000002'"),
            ("offset='0'", "offset='0.0000000000000001'"),
            ("rgb(1%", "rgb(1.0000000000000002%"),
            ("stop-opacity='0.5'", "stop-opacity='0.5000000000000001'"),
            ("M1 2", "m1 2"),
            ("C3 4", "L3 4"),
            (" 8 Z", " 8"),
            ("M1 2", "M1  2"),
            ("M1 2", "M1 2 3"),
            ("url(#g123-4)", "url(#g124-4)"),
            ("#010203", "#010204"),
            ("stroke-width='1'", "stroke-width='1.0000000000000002'"),
            (
                "stroke-opacity='0.5'",
                "stroke-opacity='0.5000000000000001'",
            ),
            ("evenodd", "nonzero"),
            ("<path d='M9 10'/>", ""),
            ("<path d='M9 10'/>", "<path d='M9 10'/><path d='M9 10'/>"),
            ("</g>", "</g></g>"),
        ] {
            let actual = expected.replacen(from, to, 1);
            assert_ne!(actual, expected, "ineffective mutation {from}");
            assert!(
                compare(&actual, expected).is_err(),
                "accepted mutation {from} -> {to}"
            );
        }
        assert!(
            compare(
                "<path d='M3 4'/><path d='M1 2'/>",
                "<path d='M1 2'/><path d='M3 4'/>"
            )
            .is_err()
        );
    }
}
