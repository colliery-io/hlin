//! The converter's rules: which units there are, and the arithmetic.
//!
//! All of it here, in the components, because none of it needs a platform: the
//! units do not change and the answer is only ever for the person typing.
//!
//! Every unit is an affine map onto its quantity's base unit,
//! `base = value × factor + offset`, which covers temperature (an offset) as
//! well as everything else (a factor alone).

/// One unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unit {
    /// Its id, for a `<select>`.
    pub id: &'static str,
    /// What it is called.
    pub name: &'static str,
    factor: f64,
    offset: f64,
}

/// Something that can be measured, and the units it is measured in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    /// Its id.
    pub id: &'static str,
    /// What it is called.
    pub name: &'static str,
    /// Its units, the base first.
    pub units: &'static [Unit],
    /// What to convert from and to until the person chooses.
    pub from: &'static str,
    /// See `from`.
    pub to: &'static str,
    /// The smallest base value there can be, if there is one.
    least: Option<(f64, &'static str)>,
}

const fn unit(id: &'static str, name: &'static str, factor: f64) -> Unit {
    Unit {
        id,
        name,
        factor,
        offset: 0.0,
    }
}

/// Everything the converter knows.
pub const QUANTITIES: &[Quantity] = &[
    Quantity {
        id: "length",
        name: "Length",
        units: &[
            unit("m", "metres", 1.0),
            unit("mm", "millimetres", 0.001),
            unit("cm", "centimetres", 0.01),
            unit("km", "kilometres", 1000.0),
            unit("in", "inches", 0.0254),
            unit("ft", "feet", 0.3048),
            unit("yd", "yards", 0.9144),
            unit("mi", "miles", 1609.344),
        ],
        from: "km",
        to: "mi",
        least: None,
    },
    Quantity {
        id: "mass",
        name: "Mass",
        units: &[
            unit("kg", "kilograms", 1.0),
            unit("g", "grams", 0.001),
            unit("t", "tonnes", 1000.0),
            unit("oz", "ounces", 0.028_349_523_125),
            unit("lb", "pounds", 0.453_592_37),
            unit("st", "stone", 6.350_293_18),
        ],
        from: "kg",
        to: "lb",
        least: None,
    },
    Quantity {
        id: "temperature",
        name: "Temperature",
        units: &[
            unit("K", "kelvin", 1.0),
            Unit {
                id: "C",
                name: "°C",
                factor: 1.0,
                offset: 273.15,
            },
            Unit {
                id: "F",
                name: "°F",
                factor: 5.0 / 9.0,
                offset: 459.67 * 5.0 / 9.0,
            },
        ],
        from: "C",
        to: "F",
        least: Some((0.0, "Nothing is colder than absolute zero")),
    },
    Quantity {
        id: "volume",
        name: "Volume",
        units: &[
            unit("l", "litres", 1.0),
            unit("ml", "millilitres", 0.001),
            unit("tsp", "teaspoons (US)", 0.004_928_921_593_75),
            unit("tbsp", "tablespoons (US)", 0.014_786_764_781_25),
            unit("floz", "fluid ounces (US)", 0.029_573_529_562_5),
            unit("cup", "cups (US)", 0.236_588_236_5),
            unit("gal", "gallons (US)", 3.785_411_784),
        ],
        from: "cup",
        to: "ml",
        least: None,
    },
    Quantity {
        id: "speed",
        name: "Speed",
        units: &[
            unit("mps", "metres a second", 1.0),
            unit("kph", "kilometres an hour", 1.0 / 3.6),
            unit("mph", "miles an hour", 0.447_04),
            unit("kn", "knots", 1852.0 / 3600.0),
        ],
        from: "kph",
        to: "mph",
        least: None,
    },
    Quantity {
        id: "data",
        name: "Data",
        units: &[
            unit("B", "bytes", 1.0),
            unit("kB", "kilobytes", 1e3),
            unit("MB", "megabytes", 1e6),
            unit("GB", "gigabytes", 1e9),
            unit("KiB", "kibibytes", 1024.0),
            unit("MiB", "mebibytes", 1_048_576.0),
            unit("GiB", "gibibytes", 1_073_741_824.0),
        ],
        from: "GB",
        to: "GiB",
        least: None,
    },
];

/// The quantity called `id`.
pub fn quantity(id: &str) -> Option<&'static Quantity> {
    QUANTITIES.iter().find(|quantity| quantity.id == id)
}

impl Quantity {
    /// The unit called `id`, if it measures this.
    pub fn unit(&self, id: &str) -> Option<&'static Unit> {
        self.units.iter().find(|unit| unit.id == id)
    }
}

/// `typed`, in `from`, as `to`: the answer, or what is wrong, in words.
pub fn convert(quantity: &Quantity, typed: &str, from: &str, to: &str) -> Result<f64, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err(String::new());
    }
    // A comma is a decimal point where there is no point already.
    let value: f64 = if typed.contains('.') {
        typed.parse()
    } else {
        typed.replacen(',', ".", 1).parse()
    }
    .ok()
    .filter(|value: &f64| value.is_finite())
    .ok_or_else(|| "That is not a number".to_string())?;
    let (Some(from), Some(to)) = (quantity.unit(from), quantity.unit(to)) else {
        return Err(format!(
            "Those are not units of {}",
            quantity.name.to_lowercase()
        ));
    };
    let base = value * from.factor + from.offset;
    if let Some((least, words)) = quantity.least
        && base < least - 1e-9
    {
        return Err(words.to_string());
    }
    Ok((base - to.offset) / to.factor)
}

/// A number as a person would write it: up to six significant figures, and
/// no trailing zeros.
pub fn written(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs().log10().floor() as i32;
    if !(-6..15).contains(&magnitude) {
        return format!("{value:.5e}");
    }
    let decimals = (5 - magnitude).max(0) as usize;
    let text = format!("{value:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_(id: &str, typed: &str, from: &str, to: &str) -> String {
        written(convert(quantity(id).unwrap(), typed, from, to).unwrap())
    }

    #[test]
    fn lengths_masses_volumes_speeds_and_data() {
        assert_eq!(in_("length", "1", "mi", "km"), "1.60934");
        assert_eq!(in_("length", "12", "in", "ft"), "1");
        assert_eq!(in_("mass", "1", "kg", "lb"), "2.20462");
        assert_eq!(in_("mass", "14", "lb", "st"), "1");
        assert_eq!(in_("volume", "1", "cup", "ml"), "236.588");
        assert_eq!(in_("speed", "100", "kph", "mph"), "62.1371");
        assert_eq!(in_("data", "1", "GiB", "MB"), "1073.74");
    }

    #[test]
    fn temperatures_have_offsets_as_well_as_scales() {
        assert_eq!(in_("temperature", "100", "C", "F"), "212");
        assert_eq!(in_("temperature", "-40", "F", "C"), "-40");
        assert_eq!(in_("temperature", "0", "K", "C"), "-273.15");
        assert_eq!(in_("temperature", "32", "F", "K"), "273.15");
    }

    #[test]
    fn nothing_is_colder_than_absolute_zero() {
        let temperature = quantity("temperature").unwrap();
        assert_eq!(
            convert(temperature, "-300", "C", "F"),
            Err("Nothing is colder than absolute zero".to_string())
        );
        // Negative lengths are just a direction.
        assert_eq!(in_("length", "-1", "km", "m"), "-1000");
    }

    #[test]
    fn what_is_typed_must_be_a_number_in_units_of_the_quantity() {
        let length = quantity("length").unwrap();
        assert_eq!(convert(length, "", "m", "km"), Err(String::new()));
        assert_eq!(
            convert(length, "ten", "m", "km"),
            Err("That is not a number".to_string())
        );
        assert_eq!(
            convert(length, "inf", "m", "km"),
            Err("That is not a number".to_string())
        );
        assert!(convert(length, "1", "kg", "km").is_err());
        assert_eq!(in_("length", " 2,5 ", "km", "m"), "2500");
    }

    #[test]
    fn every_unit_goes_there_and_back() {
        for quantity in QUANTITIES {
            assert!(quantity.unit(quantity.from).is_some(), "{}", quantity.id);
            assert!(quantity.unit(quantity.to).is_some(), "{}", quantity.id);
            for from in quantity.units {
                for to in quantity.units {
                    let there = convert(quantity, "300", from.id, to.id).unwrap();
                    let back = convert(quantity, &there.to_string(), to.id, from.id).unwrap();
                    assert!(
                        (back - 300.0).abs() < 1e-6,
                        "{} {} {}",
                        quantity.id,
                        from.id,
                        to.id
                    );
                }
            }
        }
    }

    #[test]
    fn numbers_are_written_as_a_person_would() {
        assert_eq!(written(1609.344), "1609.34");
        assert_eq!(written(2.0), "2");
        assert_eq!(written(0.000_123_456_7), "0.000123457");
        assert_eq!(written(-0.0), "0");
        assert_eq!(written(1e20), "1.00000e20");
    }
}
