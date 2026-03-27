# quasar-mobile

Mobile platform support for the Quasar Engine.

## Features

- **Touch Input**: Multi-pointer, pressure support
- **Gestures**: Tap, swipe, pinch, rotate
- **Sensors**: Gyroscope, accelerometer, magnetometer
- **Haptics**: Vibration feedback
- **Android**: JNI integration, asset manager
- **iOS**: objc2 integration

## Usage

```rust
use quasar_mobile::{MobilePlugin, TouchInput};

app.add_plugin(MobilePlugin);

// Access touch input
let touch = world.resource::<TouchInput>();
for finger in touch.fingers() {
    println!("Finger {} at ({}, {})", finger.id, finger.x, finger.y);
}
```

## Platform-Specific

### Android

- JNI bridge for Java interop
- Android asset manager integration
- Native activity support

### iOS

- objc2 for Objective-C interop
- UIKit integration
- iOS simulator support
