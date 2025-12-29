// ============================================================================
// Robot Base Keyboard Control Demo
//
// This demo shows how to control a robot base using keyboard input (WASD/QE).
// It demonstrates:
//   - WebSocket connection to the robot
//   - API initialization and session management
//   - Real-time speed control with keyboard
//   - Live feedback display with terminal UI
//
// Usage:
//   cargo run --example base-advanced-control ws://localhost:8439
//
// Controls:
//   W/S - Move forward/backward (X axis)
//   A/D - Move left/right (Y axis)
//   Q/E - Rotate left/right (Z axis)
//   ESC/C - Exit
// ============================================================================

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use std::sync::Arc;
use std::sync::Mutex;
use tokio_tungstenite::MaybeTlsStream;

const ACCEPTABLE_PROTOCOL_MAJOR_VERSION: u32 = 1;

// Import our UI and keyboard modules
#[path = "lib/keyboard_input.rs"]
mod keyboard_input;
#[path = "lib/robot_ui.rs"]
mod robot_ui;

use crate::keyboard_input::{KeyboardInput, SpeedData};
use crate::robot_ui::{ControlState, ErrorMessage, RobotUi};

#[derive(Parser)]
struct Args {
    #[arg(help = "WebSocket URL to connect to (e.g. ws://localhost:8439)")]
    url: String,
}

// Speed Configuration - Modify these values to change robot speed
// Linear speed for X and Y axes (forward/backward and left/right)
const LINEAR_SPEED: f32 = 0.1; // m/s

/// Angular speed for Z axis (rotation)
const ANGULAR_SPEED: f32 = 0.5; // rad/s

pub mod base_backend {
    include!(concat!(env!("OUT_DIR"), "/_.rs"));
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize UI and keyboard input
    let mut ui = RobotUi::new().expect("Failed to initialize UI");
    let keyboard = KeyboardInput::new(LINEAR_SPEED, ANGULAR_SPEED)
        .expect("Failed to initialize keyboard input");

    // Connect to WebSocket
    let res = tokio_tungstenite::connect_async(&args.url).await;
    let ws_stream = match res {
        Ok((ws, _)) => ws,
        Err(e) => {
            ui.cleanup().ok();
            eprintln!("Error during websocket handshake: {}", e);
            return;
        }
    };

    // Set TCP nodelay for better performance
    if let MaybeTlsStream::Plain(stream) = ws_stream.get_ref() {
        stream.set_nodelay(true).unwrap();
    }
    let (mut ws_sink, ws_stream) = ws_stream.split();

    //Initialize shared state
    let control_state = Arc::new(Mutex::new(ControlState::Uninitialized));
    let odometry_data = Arc::new(Mutex::new(None));
    let emergency_stop = Arc::new(Mutex::new(false));
    let error_message = Arc::new(Mutex::new(ErrorMessage::default()));

    //Spawn WebSocket receiver task
    spawn_websocket_receiver(
        ws_stream,
        control_state.clone(),
        odometry_data.clone(),
        emergency_stop.clone(),
        error_message.clone(),
    );

    //Spawn Ctrl-C handler
    let keyboard_clone = Arc::new(keyboard);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        std::process::exit(0);
    });

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Get current state
        let current_state = *control_state.lock().unwrap();
        let target_speed = keyboard_clone.get_speed();
        let actual_speed = *odometry_data.lock().unwrap();
        let pressed_keys = keyboard_clone.get_pressed_keys();
        let error_msg = error_message.lock().unwrap().clone();
        let emergency = *emergency_stop.lock().unwrap();

        // Draw UI
        let _ = ui
            .draw(
                current_state,
                &target_speed,
                actual_speed,
                &pressed_keys,
                &error_msg,
                emergency,
            )
            .is_err();

        // Check if we should exit
        if keyboard_clone.should_exit() {
            // Send API close command
            let close_message = create_close_msg();
            let close_bytes = close_message.encode_to_vec();

            ws_sink
                .send(tungstenite::Message::Binary(close_bytes.into()))
                .await
                .ok();

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            break;
        }

        // State machine logic - send appropriate commands based on state
        match current_state {
            ControlState::Uninitialized => {
                // Set report frequency to 50Hz
                let set_freq_msg = create_set_frequency_msg(base_backend::ReportFrequency::Rf50Hz);
                let set_freq_bytes = set_freq_msg.encode_to_vec();
                if ws_sink
                    .send(tungstenite::Message::Binary(set_freq_bytes.into()))
                    .await
                    .is_err()
                {
                    break;
                }

                // Initialize the base API control
                let enable_message = create_init_msg();
                let enable_bytes = enable_message.encode_to_vec();

                if ws_sink
                    .send(tungstenite::Message::Binary(enable_bytes.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }

            ControlState::CanMove => {
                // Send move command with current target speed
                let move_message = create_move_msg(target_speed.x, target_speed.y, target_speed.z);
                let move_bytes = move_message.encode_to_vec();

                if ws_sink
                    .send(tungstenite::Message::Binary(move_bytes.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }

            ControlState::InitializedButNotHold => {
                // Wait for control authority
                continue;
            }
        }
    }
}

// WebSocket Message Handlers
// Process base status messages and update state
fn handle_base_status(
    base_status: &base_backend::BaseStatus,
    session_id: u32,
    odometry_data: Arc<Mutex<Option<SpeedData>>>,
    emergency_stop: Arc<Mutex<bool>>,
    error_message: Arc<Mutex<ErrorMessage>>,
) -> ControlState {
    // Check for parking/emergency stop
    let parking = base_status.parking_stop_detail.is_some();
    if let Some(ref parking_detail) = base_status.parking_stop_detail {
        let msg = format!("Emergency Stop: {:?}", parking_detail);
        *error_message.lock().unwrap() = ErrorMessage::new(msg);
        *emergency_stop.lock().unwrap() = true;
    } else {
        *emergency_stop.lock().unwrap() = false;
        // Clear error message after 3 seconds
        let mut err = error_message.lock().unwrap();
        if err.is_expired(std::time::Duration::from_secs(3)) {
            *err = ErrorMessage::default();
        }
    }
    let session_holder = base_status.session_holder;
    let api_initialized = base_status.api_control_initialized;

    // Determine control state
    let state = if !api_initialized {
        ControlState::Uninitialized
    } else if !parking && session_holder == session_id {
        ControlState::CanMove
    } else {
        ControlState::InitializedButNotHold
    };

    // Update odometry data if we have control
    if state == ControlState::CanMove {
        if let Some(ref estimated_odometry) = base_status.estimated_odometry {
            *odometry_data.lock().unwrap() = Some(SpeedData {
                x: estimated_odometry.speed_x,
                y: estimated_odometry.speed_y,
                z: estimated_odometry.speed_z,
            });
        }
    }

    state
}

// Spawn task to receive and process WebSocket messages
fn spawn_websocket_receiver(
    mut ws_stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    control_state: Arc<Mutex<ControlState>>,
    odometry_data: Arc<Mutex<Option<SpeedData>>>,
    emergency_stop: Arc<Mutex<bool>>,
    error_message: Arc<Mutex<ErrorMessage>>,
) {
    tokio::spawn(async move {
        while let Some(msg) = ws_stream.next().await {
            let msg = msg.unwrap();
            if let tungstenite::Message::Binary(bytes) = msg {
                let msg = base_backend::ApiUp::decode(bytes).unwrap();
                if let Some(log) = msg.log {
                    *error_message.lock().unwrap() = ErrorMessage::new(format!("Log: {:?}", log));
                }
                let session_id = msg.session_id;
                let protocol_version = msg.protocol_major_version;
                if let Some(base_backend::api_up::Status::BaseStatus(base_status)) = msg.status {
                    let state = handle_base_status(
                        &base_status,
                        session_id,
                        odometry_data.clone(),
                        emergency_stop.clone(),
                        error_message.clone(),
                    );
                    *control_state.lock().unwrap() = state;
                    // Only show control loss message when actually losing control
                    if state == ControlState::InitializedButNotHold {
                        *error_message.lock().unwrap() =
                            ErrorMessage::new("Control in hands of another user".to_string());
                    }
                    if state == ControlState::CanMove
                        && protocol_version != ACCEPTABLE_PROTOCOL_MAJOR_VERSION
                    {
                        *error_message.lock().unwrap() =
                            ErrorMessage::new("Protocol version mismatch".to_string());
                    }
                }
            };
        }
    });
}

// Message Creation Helpers
//Create a message to set the report frequency
fn create_set_frequency_msg(frequency: base_backend::ReportFrequency) -> base_backend::ApiDown {
    base_backend::ApiDown {
        down: Some(base_backend::api_down::Down::SetReportFrequency(
            frequency as i32,
        )),
    }
}

// Create a message to initialize API control
fn create_init_msg() -> base_backend::ApiDown {
    base_backend::ApiDown {
        down: Some(base_backend::api_down::Down::BaseCommand(
            base_backend::BaseCommand {
                command: Some(base_backend::base_command::Command::ApiControlInitialize(
                    true,
                )),
            },
        )),
    }
}

// Create a message to send move commands
// Users can easily see and modify speed parameters here
fn create_move_msg(speed_x: f32, speed_y: f32, speed_z: f32) -> base_backend::ApiDown {
    base_backend::ApiDown {
        down: Some(base_backend::api_down::Down::BaseCommand(
            base_backend::BaseCommand {
                command: Some(base_backend::base_command::Command::SimpleMoveCommand(
                    base_backend::SimpleBaseMoveCommand {
                        command: Some(base_backend::simple_base_move_command::Command::XyzSpeed(
                            base_backend::XyzSpeed {
                                speed_x,
                                speed_y,
                                speed_z,
                            },
                        )),
                    },
                )),
            },
        )),
    }
}

//Create a message to close/disable API control
fn create_close_msg() -> base_backend::ApiDown {
    base_backend::ApiDown {
        down: Some(base_backend::api_down::Down::BaseCommand(
            base_backend::BaseCommand {
                command: Some(base_backend::base_command::Command::ApiControlInitialize(
                    false,
                )),
            },
        )),
    }
}
