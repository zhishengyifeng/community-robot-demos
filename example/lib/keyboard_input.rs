// ============================================================================
// Keyboard Input Module - Handles keyboard events and speed control
// ============================================================================

use crossterm::{event::{self,Event, KeyCode}};
use std::{collections::{HashMap}, time::Instant};
use std::sync::{Arc, Mutex};

// Speed data structure for X, Y, Z axes
#[derive(Clone, Copy, Debug, Default)]
pub struct SpeedData {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone)]
pub struct KeyState {
    last_seen: Instant,
    is_holding: bool,
}

//Keyboard input handler - encapsulates all keyboard processing complexity
pub struct KeyboardInput {
    speed: Arc<Mutex<SpeedData>>,
    should_exit: Arc<Mutex<bool>>,
    pressed_keys: Arc<Mutex<HashMap<KeyCode,KeyState>>>,
    linear_speed: f32,
    angular_speed: f32,
}

impl KeyboardInput {
    pub fn new(linear_speed: f32, angular_speed: f32) -> std::io::Result<Self> {
        let input = Self {
            speed: Arc::new(Mutex::new(SpeedData::default())),
            should_exit: Arc::new(Mutex::new(false)),
            pressed_keys: Arc::new(Mutex::new(HashMap::new())),
            linear_speed,
            angular_speed,
        };
        input.spawn_handler();
        Ok(input)
    }

    pub fn get_speed(&self) -> SpeedData {
        self.speed.lock().unwrap().clone()
    }
    
    pub fn should_exit(&self) -> bool {
        *self.should_exit.lock().unwrap()
    }

    pub fn get_pressed_keys(&self) -> HashMap<KeyCode, KeyState> {
        self.pressed_keys.lock().unwrap().clone()
    }

    fn spawn_handler(&self) {
        let speed = self.speed.clone();
        let should_exit = self.should_exit.clone();
        let pressed_keys = self.pressed_keys.clone();
        let linear_speed = self.linear_speed;
        let angular_speed = self.angular_speed;
        let mut release_time = std::time::Duration::from_millis(100);
        tokio::spawn(async move {
            loop {
                match event::poll(std::time::Duration::from_millis(50)) {
                    Ok(has_event) => {
                        if has_event {
                            if let Event::Key(key_event) = event::read().unwrap(){
                                let key_code = key_event.code;
                                
                                if key_code == KeyCode::Char('c') {
                                    *should_exit.lock().unwrap() = true;
                                    break;
                                }

                                let mut keys = pressed_keys.lock().unwrap();
                                match keys.get_mut(&key_code) {
                                    Some(key_state) => {
                                        if !key_state.is_holding {
                                            key_state.is_holding = true;
                                        }
                                        release_time = std::time::Duration::from_millis(100);
                                        key_state.last_seen = Instant::now();
                                    }
                                    None => {
                                        release_time = std::time::Duration::from_millis(500);
                                        keys.insert(key_code, KeyState {
                                            last_seen: Instant::now(),
                                            is_holding: false,
                                        });
                                    }
                                }
                            }
                        } else {
                            let mut keys = pressed_keys.lock().unwrap();
                            let mut released_keys = Vec::new();
                            let now = Instant::now();

                            for (key, state) in keys.iter() {
                                if now.duration_since(state.last_seen) > release_time {
                                    released_keys.push(*key);
                                }
                            }

                            for key in released_keys {
                                keys.remove(&key);
                            }

                        }
                    }
                    Err(_) => break,
                }
                    Self::update_speed(&speed,&pressed_keys, linear_speed, angular_speed);
            }
        });
    }

    fn update_speed(
        speed: &Arc<Mutex<SpeedData>>,
        keys: &Arc<Mutex<HashMap<KeyCode,KeyState>>>,
        linear_speed: f32,
        angular_speed: f32,
    ){
        let key = keys.lock().unwrap();
        let mut spd = speed.lock().unwrap();

        spd.x = if key.contains_key(&KeyCode::Char('w')) {
            linear_speed
        }else if key.contains_key(&KeyCode::Char('s')) {
            -linear_speed
        }else {
            0.0
        };

        spd.y = if key.contains_key(&KeyCode::Char('d')) {
            linear_speed
        }else if key.contains_key(&KeyCode::Char('a')) {
            -linear_speed
        }else {
            0.0
        };

        spd.z = if key.contains_key(&KeyCode::Char('q')) {
            angular_speed
        }else if key.contains_key(&KeyCode::Char('e')) {
            -angular_speed
        }else {
            0.0
        };
    }

}