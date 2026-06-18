pub(crate) use shared::dark_sky::{
    classify_dark_sky, DarkSkyClass, DARK_SKY_CLASSIFICATION_VERSION,
};

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
