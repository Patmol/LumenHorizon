use serde::Serialize;

pub const DARK_SKY_CLASSIFICATION_VERSION: &str = "radiance-dark-sky-v1";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DarkSkyClass {
    pub class: u8,
    pub color_hex: &'static str,
    pub label: &'static str,
    pub min_radiance: f32,
    pub max_radiance_exclusive: Option<f32>,
}

pub const DARK_SKY_CLASSES: &[DarkSkyClass] = &[
    DarkSkyClass {
        class: 1,
        color_hex: "#05070d",
        label: "Excellent dark site",
        min_radiance: 0.0,
        max_radiance_exclusive: Some(0.2),
    },
    DarkSkyClass {
        class: 2,
        color_hex: "#10203f",
        label: "Dark site",
        min_radiance: 0.2,
        max_radiance_exclusive: Some(0.4),
    },
    DarkSkyClass {
        class: 3,
        color_hex: "#1f3f75",
        label: "Rural sky",
        min_radiance: 0.4,
        max_radiance_exclusive: Some(1.0),
    },
    DarkSkyClass {
        class: 4,
        color_hex: "#2f6f73",
        label: "Rural/suburban transition",
        min_radiance: 1.0,
        max_radiance_exclusive: Some(3.0),
    },
    DarkSkyClass {
        class: 5,
        color_hex: "#62a35c",
        label: "Suburban edge",
        min_radiance: 3.0,
        max_radiance_exclusive: Some(6.0),
    },
    DarkSkyClass {
        class: 6,
        color_hex: "#b6b34b",
        label: "Bright suburban",
        min_radiance: 6.0,
        max_radiance_exclusive: Some(12.0),
    },
    DarkSkyClass {
        class: 7,
        color_hex: "#d9822b",
        label: "Urban transition",
        min_radiance: 12.0,
        max_radiance_exclusive: Some(25.0),
    },
    DarkSkyClass {
        class: 8,
        color_hex: "#d64a2f",
        label: "City sky",
        min_radiance: 25.0,
        max_radiance_exclusive: Some(50.0),
    },
    DarkSkyClass {
        class: 9,
        color_hex: "#f2efe8",
        label: "Inner-city sky",
        min_radiance: 50.0,
        max_radiance_exclusive: None,
    },
];

pub fn classify_dark_sky(radiance: f32) -> Option<DarkSkyClass> {
    if !radiance.is_finite() || radiance < 0.0 {
        return None;
    }

    DARK_SKY_CLASSES
        .iter()
        .copied()
        .find(|class| match class.max_radiance_exclusive {
            Some(max) => radiance >= class.min_radiance && radiance < max,
            None => radiance >= class.min_radiance,
        })
}

#[cfg(test)]
mod tests {
    use super::{classify_dark_sky, DARK_SKY_CLASSIFICATION_VERSION};

    #[test]
    fn classifies_dark_sky_radiance_boundaries() {
        let cases = [
            (0.0, 1),
            (0.1999, 1),
            (0.2, 2),
            (0.3999, 2),
            (0.4, 3),
            (0.9999, 3),
            (1.0, 4),
            (2.9999, 4),
            (3.0, 5),
            (5.9999, 5),
            (6.0, 6),
            (11.9999, 6),
            (12.0, 7),
            (24.9999, 7),
            (25.0, 8),
            (49.9999, 8),
            (50.0, 9),
        ];

        for (radiance, expected_class) in cases {
            assert_eq!(
                classify_dark_sky(radiance).map(|class| class.class),
                Some(expected_class),
                "radiance {radiance} should classify as {expected_class}"
            );
        }
    }

    #[test]
    fn rejects_invalid_dark_sky_radiance_values() {
        for radiance in [f32::NAN, f32::INFINITY, -0.1] {
            assert_eq!(classify_dark_sky(radiance), None);
        }
    }

    #[test]
    fn exposes_dark_sky_classification_version() {
        assert_eq!(DARK_SKY_CLASSIFICATION_VERSION, "radiance-dark-sky-v1");
    }
}
