use super::{CodeSpace, Geometry, Storage};
/// Named conventional layouts. Explicit geometry can override every preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// 12-pixel square, 16 storage bits per row.
    Hzk12,
    /// 14-pixel square, 16 storage bits per row.
    Hzk14,
    /// 16-pixel square.
    Hzk16,
    /// Traditional-glyph HZK16 variant, identical wire geometry.
    Hzk16F,
    /// HZK16 style variant, identical wire geometry.
    Hzk16S,
    /// 24-pixel UCDOS print layout, zone 16, transposed.
    Hzk24,
    /// 24-pixel Song print layout.
    Hzk24S,
    /// 24-pixel Kai print layout.
    Hzk24K,
    /// 24-pixel Hei print layout.
    Hzk24H,
    /// 24-pixel Fang Song print layout.
    Hzk24F,
    /// 24-pixel print symbols, zones 1–15, transposed.
    Hzk24T,
    /// 32-pixel square, row-major.
    Hzk32,
    /// 40-pixel square, row-major.
    Hzk40,
    /// 40-pixel Hanzi, begins zone 16.
    Hzk40S,
    /// 40-pixel symbols, zones 1–15.
    Hzk40T,
    /// 48-pixel square, row-major.
    Hzk48,
    /// 48-pixel Hanzi, begins zone 16.
    Hzk48S,
    /// 48-pixel symbols, zones 1–15.
    Hzk48T,
    /// 8×12 single-byte cells.
    Asc12,
    /// 8×14 single-byte cells.
    Asc14,
    /// 8×16 single-byte cells.
    Asc16,
    /// 12×24 single-byte cells.
    Asc24,
    /// 16×32 single-byte cells.
    Asc32,
    /// 20×40 single-byte cells.
    Asc40,
    /// 24×48 single-byte cells.
    Asc48,
}
impl Preset {
    /// All documented presets in stable order.
    pub const ALL: &'static [Self] = &[
        Self::Hzk12,
        Self::Hzk14,
        Self::Hzk16,
        Self::Hzk16F,
        Self::Hzk16S,
        Self::Hzk24,
        Self::Hzk24S,
        Self::Hzk24K,
        Self::Hzk24H,
        Self::Hzk24F,
        Self::Hzk24T,
        Self::Hzk32,
        Self::Hzk40,
        Self::Hzk40S,
        Self::Hzk40T,
        Self::Hzk48,
        Self::Hzk48S,
        Self::Hzk48T,
        Self::Asc12,
        Self::Asc14,
        Self::Asc16,
        Self::Asc24,
        Self::Asc32,
        Self::Asc40,
        Self::Asc48,
    ];
    /// CLI spelling, matched case-insensitively by `FromStr`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hzk12 => "hzk12",
            Self::Hzk14 => "hzk14",
            Self::Hzk16 => "hzk16",
            Self::Hzk16F => "hzk16f",
            Self::Hzk16S => "hzk16s",
            Self::Hzk24 => "hzk24",
            Self::Hzk24S => "hzk24s",
            Self::Hzk24K => "hzk24k",
            Self::Hzk24H => "hzk24h",
            Self::Hzk24F => "hzk24f",
            Self::Hzk24T => "hzk24t",
            Self::Hzk32 => "hzk32",
            Self::Hzk40 => "hzk40",
            Self::Hzk40S => "hzk40s",
            Self::Hzk40T => "hzk40t",
            Self::Hzk48 => "hzk48",
            Self::Hzk48S => "hzk48s",
            Self::Hzk48T => "hzk48t",
            Self::Asc12 => "asc12",
            Self::Asc14 => "asc14",
            Self::Asc16 => "asc16",
            Self::Asc24 => "asc24",
            Self::Asc32 => "asc32",
            Self::Asc40 => "asc40",
            Self::Asc48 => "asc48",
        }
    }
    /// Complete, inspectable wire geometry.
    pub const fn geometry(self) -> Geometry {
        let (width, height, ascii) = match self {
            Self::Hzk12 => (12, 12, false),
            Self::Hzk14 => (14, 14, false),
            Self::Hzk16 | Self::Hzk16F | Self::Hzk16S => (16, 16, false),
            Self::Hzk24
            | Self::Hzk24S
            | Self::Hzk24K
            | Self::Hzk24H
            | Self::Hzk24F
            | Self::Hzk24T => (24, 24, false),
            Self::Hzk32 => (32, 32, false),
            Self::Hzk40 | Self::Hzk40S | Self::Hzk40T => (40, 40, false),
            Self::Hzk48 | Self::Hzk48S | Self::Hzk48T => (48, 48, false),
            Self::Asc12 => (8, 12, true),
            Self::Asc14 => (8, 14, true),
            Self::Asc16 => (8, 16, true),
            Self::Asc24 => (12, 24, true),
            Self::Asc32 => (16, 32, true),
            Self::Asc40 => (20, 40, true),
            Self::Asc48 => (24, 48, true),
        };
        let print = matches!(
            self,
            Self::Hzk24
                | Self::Hzk24S
                | Self::Hzk24K
                | Self::Hzk24H
                | Self::Hzk24F
                | Self::Hzk40S
                | Self::Hzk48S
        );
        let symbols = matches!(self, Self::Hzk24T | Self::Hzk40T | Self::Hzk48T);
        Geometry {
            width,
            height,
            first_code: if ascii {
                0
            } else if print {
                0xb0a1
            } else {
                0xa1a1
            },
            last_code: if ascii {
                255
            } else if symbols {
                0xaffe
            } else {
                0xfefe
            },
            code_space: if ascii {
                CodeSpace::Ascii
            } else {
                CodeSpace::Gb2312
            },
            storage: if matches!(
                self,
                Self::Hzk24
                    | Self::Hzk24S
                    | Self::Hzk24K
                    | Self::Hzk24H
                    | Self::Hzk24F
                    | Self::Hzk24T
            ) {
                Storage::Columns
            } else {
                Storage::Rows
            },
        }
    }
}
impl std::str::FromStr for Preset {
    type Err = crate::Error;
    fn from_str(s: &str) -> crate::Result<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|p| p.name().eq_ignore_ascii_case(s))
            .ok_or_else(|| crate::Error::InvalidInput(format!("Unknown HZK/ASC preset {s:?}")))
    }
}
