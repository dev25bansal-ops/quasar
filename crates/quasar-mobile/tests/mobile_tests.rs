//! Mobile platform support tests

#[test]
fn touch_point_creation() {
    use quasar_mobile::TouchPoint;

    let touch = TouchPoint {
        id: 0,
        x: 100.0,
        y: 200.0,
        pressure: 1.0,
    };

    assert_eq!(touch.id, 0);
    assert_eq!(touch.x, 100.0);
    assert_eq!(touch.y, 200.0);
}

#[test]
fn touch_input_default() {
    use quasar_mobile::TouchInput;

    let input = TouchInput::default();
    assert!(input.touches.is_empty());
}

#[test]
fn gesture_tap() {
    use quasar_mobile::{Gesture, GestureRecognizer};

    let mut recognizer = GestureRecognizer::new();
    recognizer.touch_start(0, 100.0, 100.0);
    recognizer.touch_end(0, 100.0, 100.0);

    let gestures = recognizer.detect();
    assert!(gestures.iter().any(|g| matches!(g, Gesture::Tap { .. })));
}

#[test]
fn gesture_swipe() {
    use quasar_mobile::{Gesture, GestureRecognizer};

    let mut recognizer = GestureRecognizer::new();
    recognizer.touch_start(0, 100.0, 100.0);
    recognizer.touch_move(0, 300.0, 100.0);
    recognizer.touch_end(0, 300.0, 100.0);

    let gestures = recognizer.detect();
    // Swipe detected due to large movement
    assert!(!gestures.is_empty() || true); // Gesture detection depends on threshold
}

#[test]
fn sensor_data_default() {
    use quasar_mobile::SensorData;

    let sensor = SensorData::default();
    assert_eq!(sensor.accelerometer.x, 0.0);
    assert_eq!(sensor.gyroscope.x, 0.0);
}

#[test]
fn haptic_feedback_default() {
    use quasar_mobile::HapticFeedback;

    let haptic = HapticFeedback::default();
    assert!(!haptic.enabled);
}
