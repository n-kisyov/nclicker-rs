use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, VK_F1, VK_F2, VK_F3, VK_F4,
    VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12
};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
use std::ptr;

// Use winapi for everything related to hotkeys
use winapi::um::winuser::{RegisterHotKey, UnregisterHotKey, GetMessageW, MSG, WM_HOTKEY};
use winapi::um::winuser::{MOD_ALT, MOD_CONTROL, MOD_SHIFT};

const HOTKEY_START_STOP: i32 = 1;
const HOTKEY_STOP_ONLY: i32 = 2;

#[derive(Clone, Copy, PartialEq)]
enum MouseButton {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
enum ClickMode {
    RepeatCount(u32),
    RepeatUntilStopped,
}

#[derive(Clone, Copy, PartialEq)]
enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ModifierKey {
    None,
    Alt,
    Ctrl,
    Shift,
    AltCtrl,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum FunctionKey {
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

impl ModifierKey {
    fn to_windows_mod(&self) -> u32 {
        match self {
            ModifierKey::None => 0,
            ModifierKey::Alt => MOD_ALT as u32,
            ModifierKey::Ctrl => MOD_CONTROL as u32,
            ModifierKey::Shift => MOD_SHIFT as u32,
            ModifierKey::AltCtrl => (MOD_ALT | MOD_CONTROL) as u32,
        }
    }
    
    fn to_string(&self) -> String {
        match self {
            ModifierKey::None => "".to_string(),
            ModifierKey::Alt => "Alt+".to_string(),
            ModifierKey::Ctrl => "Ctrl+".to_string(),
            ModifierKey::Shift => "Shift+".to_string(),
            ModifierKey::AltCtrl => "Alt+Ctrl+".to_string(),
        }
    }
}

impl FunctionKey {
    fn to_windows_vk(&self) -> u32 {
        match self {
            FunctionKey::F1 => VK_F1.0 as u32,
            FunctionKey::F2 => VK_F2.0 as u32,
            FunctionKey::F3 => VK_F3.0 as u32,
            FunctionKey::F4 => VK_F4.0 as u32,
            FunctionKey::F5 => VK_F5.0 as u32,
            FunctionKey::F6 => VK_F6.0 as u32,
            FunctionKey::F7 => VK_F7.0 as u32,
            FunctionKey::F8 => VK_F8.0 as u32,
            FunctionKey::F9 => VK_F9.0 as u32,
            FunctionKey::F10 => VK_F10.0 as u32,
            FunctionKey::F11 => VK_F11.0 as u32,
            FunctionKey::F12 => VK_F12.0 as u32,
        }
    }
    
    fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

#[derive(Clone)]
struct ClickerState {
    is_running: Arc<Mutex<bool>>,
    click_count: Arc<Mutex<u32>>,
    should_start: Arc<Mutex<bool>>,
    should_stop: Arc<Mutex<bool>>,
    hotkey_thread_running: Arc<Mutex<bool>>,
}

impl ClickerState {
    fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            click_count: Arc::new(Mutex::new(0)),
            should_start: Arc::new(Mutex::new(false)),
            should_stop: Arc::new(Mutex::new(false)),
            hotkey_thread_running: Arc::new(Mutex::new(false)),
        }
    }
    
    fn start(&self) {
        *self.is_running.lock().unwrap() = true;
        *self.click_count.lock().unwrap() = 0;
    }
    
    fn stop(&self) {
        *self.is_running.lock().unwrap() = false;
    }
    
    fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }
    
    fn get_click_count(&self) -> u32 {
        *self.click_count.lock().unwrap()
    }
    
    fn increment_click_count(&self) {
        *self.click_count.lock().unwrap() += 1;
    }
    
    fn request_start(&self) {
        *self.should_start.lock().unwrap() = true;
    }
    
    fn request_stop(&self) {
        *self.should_stop.lock().unwrap() = true;
    }
    
    fn check_and_clear_start_request(&self) -> bool {
        let mut should_start = self.should_start.lock().unwrap();
        if *should_start {
            *should_start = false;
            true
        } else {
            false
        }
    }
    
    fn check_and_clear_stop_request(&self) -> bool {
        let mut should_stop = self.should_stop.lock().unwrap();
        if *should_stop {
            *should_stop = false;
            true
        } else {
            false
        }
    }
    
    fn is_hotkey_thread_running(&self) -> bool {
        *self.hotkey_thread_running.lock().unwrap()
    }
    
    fn set_hotkey_thread_running(&self, running: bool) {
        *self.hotkey_thread_running.lock().unwrap() = running;
    }
}

struct HotkeyManager {
    registered: bool,
    status: String,
}

impl HotkeyManager {
    fn new() -> Self {
        Self {
            registered: false,
            status: "Ready to register hotkeys".to_string(),
        }
    }
    
    fn register(&mut self, start_mod: ModifierKey, start_key: FunctionKey, stop_mod: ModifierKey, stop_key: FunctionKey) -> Result<(), String> {
        if self.registered {
            self.unregister();
        }
        
        unsafe {
            let result1 = RegisterHotKey(
                ptr::null_mut(),
                HOTKEY_START_STOP,
                start_mod.to_windows_mod(),
                start_key.to_windows_vk()
            );
            
            let result2 = RegisterHotKey(
                ptr::null_mut(),
                HOTKEY_STOP_ONLY,
                stop_mod.to_windows_mod(),
                stop_key.to_windows_vk()
            );
            
            if result1 != 0 && result2 != 0 {
                self.registered = true;
                self.status = format!("✅ Global hotkeys registered: {}{} / {}{}", 
                    start_mod.to_string(), start_key.to_string(),
                    stop_mod.to_string(), stop_key.to_string());
                Ok(())
            } else {
                // Try to clean up any partial registration
                let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_START_STOP);
                let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_STOP_ONLY);
                self.status = "❌ Failed to register hotkeys - they may be in use by another application".to_string();
                Err("Failed to register hotkeys. They may be in use by another application.".to_string())
            }
        }
    }
    
    fn unregister(&mut self) {
        if self.registered {
            unsafe {
                let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_START_STOP);
                let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_STOP_ONLY);
            }
            self.registered = false;
            self.status = "Hotkeys unregistered".to_string();
        }
    }
    
    fn is_registered(&self) -> bool {
        self.registered
    }
    
    fn get_status(&self) -> &str {
        &self.status
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.unregister();
    }
}

struct NClickerApp {
    // Click interval settings
    hours: u32,
    minutes: u32,
    seconds: u32,
    milliseconds: u32,
    
    // Random offset
    random_offset: bool,
    random_offset_ms: u32,
    
    // Click options
    mouse_button: MouseButton,
    click_type: String,
    
    // Click repeat settings
    click_mode: ClickMode,
    repeat_count: u32,
    
    // Cursor position
    use_current_position: bool,
    cursor_x: i32,
    cursor_y: i32,
    
    // UI Theme
    current_theme: Theme,
    
    // Hotkeys
    hotkeys_enabled: bool,
    start_modifier: ModifierKey,
    start_key: FunctionKey,
    stop_modifier: ModifierKey,
    stop_key: FunctionKey,
    show_hotkey_dialog: bool,
    
    // State
    clicker_state: ClickerState,
    hotkey_manager: HotkeyManager,
}

impl Default for NClickerApp {
    fn default() -> Self {
        Self {
            hours: 0,
            minutes: 0,
            seconds: 1,  // Default to 1 second
            milliseconds: 0,
            random_offset: false,
            random_offset_ms: 100,
            mouse_button: MouseButton::Left,
            click_type: "Single".to_string(),
            click_mode: ClickMode::RepeatUntilStopped,
            repeat_count: 1,
            use_current_position: true,
            cursor_x: 0,
            cursor_y: 0,
            current_theme: Theme::Light,
            hotkeys_enabled: true,
            start_modifier: ModifierKey::None,
            start_key: FunctionKey::F6,
            stop_modifier: ModifierKey::None,
            stop_key: FunctionKey::F7,
            show_hotkey_dialog: false,
            clicker_state: ClickerState::new(),
            hotkey_manager: HotkeyManager::new(),
        }
    }
}

impl NClickerApp {
    fn calculate_interval_ms(&self) -> u64 {
        let total_ms = (self.hours as u64 * 3600 + self.minutes as u64 * 60 + self.seconds as u64) * 1000 
                      + self.milliseconds as u64;
        if total_ms == 0 { 100 } else { total_ms }
    }
    
    fn get_start_hotkey_string(&self) -> String {
        format!("{}{}", self.start_modifier.to_string(), self.start_key.to_string())
    }
    
    fn get_stop_hotkey_string(&self) -> String {
        format!("{}{}", self.stop_modifier.to_string(), self.stop_key.to_string())
    }
    
    fn register_hotkeys(&mut self) {
        if !self.hotkeys_enabled {
            return;
        }
        
        match self.hotkey_manager.register(self.start_modifier, self.start_key, self.stop_modifier, self.stop_key) {
            Ok(()) => {
                // Start hotkey listener thread if not already started
                if !self.clicker_state.is_hotkey_thread_running() {
                    let clicker_state = self.clicker_state.clone();
                    clicker_state.set_hotkey_thread_running(true);
                    
                    thread::spawn(move || {
                        // Use winapi for the entire message loop
                        let mut msg: MSG = unsafe { std::mem::zeroed() };
                        
                        loop {
                            unsafe {
                                // Get messages for this thread
                                let result = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
                                
                                if result == 0 || result == -1 {
                                    // WM_QUIT or error
                                    break;
                                }
                                
                                if msg.message == WM_HOTKEY {
                                    match msg.wParam as i32 {
                                        HOTKEY_START_STOP => {
                                            if clicker_state.is_running() {
                                                clicker_state.request_stop();
                                            } else {
                                                clicker_state.request_start();
                                            }
                                        }
                                        HOTKEY_STOP_ONLY => {
                                            clicker_state.request_stop();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            
                            // Small sleep to prevent high CPU usage
                            thread::sleep(Duration::from_millis(1));
                        }
                        
                        clicker_state.set_hotkey_thread_running(false);
                    });
                }
            }
            Err(_) => {
                // Error already handled by hotkey_manager
            }
        }
    }
    
    fn unregister_hotkeys(&mut self) {
        self.hotkey_manager.unregister();
    }
    
    fn start_clicking(&mut self) {
        if self.clicker_state.is_running() {
            return;
        }
        
        self.clicker_state.start();
        let clicker_state = self.clicker_state.clone();
        let interval_ms = self.calculate_interval_ms();
        let mouse_button = self.mouse_button;
        let click_type = self.click_type.clone();
        let click_mode = self.click_mode;
        let use_current_position = self.use_current_position;
        let cursor_x = self.cursor_x;
        let cursor_y = self.cursor_y;
        let random_offset = self.random_offset;
        let random_offset_ms = self.random_offset_ms;
        
        thread::spawn(move || {
            let mut clicks_performed = 0;
            
            while clicker_state.is_running() {
                // Check if we should stop based on repeat count
                if let ClickMode::RepeatCount(max_clicks) = click_mode {
                    if clicks_performed >= max_clicks {
                        break;
                    }
                }
                
                // Set cursor position if needed
                unsafe {
                    if !use_current_position {
                        let _ = SetCursorPos(cursor_x, cursor_y);
                        thread::sleep(Duration::from_millis(10));
                    }
                    
                    // Perform click
                    match mouse_button {
                        MouseButton::Left => {
                            let _ = mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                            let _ = mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                            
                            if click_type == "Double" {
                                thread::sleep(Duration::from_millis(10));
                                let _ = mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                                let _ = mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                            }
                        }
                        MouseButton::Right => {
                            let _ = mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
                            let _ = mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
                            
                            if click_type == "Double" {
                                thread::sleep(Duration::from_millis(10));
                                let _ = mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
                                let _ = mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
                            }
                        }
                    }
                }
                
                clicks_performed += 1;
                clicker_state.increment_click_count();
                
                // Calculate sleep duration with optional random offset
                let mut sleep_duration = interval_ms;
                if random_offset && random_offset_ms > 0 {
                    let offset = fastrand::u32(0..=random_offset_ms);
                    sleep_duration = sleep_duration.saturating_add(offset as u64);
                }
                
                thread::sleep(Duration::from_millis(sleep_duration));
            }
            
            clicker_state.stop();
        });
    }
    
    fn stop_clicking(&mut self) {
        self.clicker_state.stop();
    }
    
    fn apply_theme(&self, ctx: &egui::Context) {
        match self.current_theme {
            Theme::Light => {
                ctx.set_visuals(egui::Visuals::light());
            },
            Theme::Dark => {
                ctx.set_visuals(egui::Visuals::dark());
            },
        }
    }
}

impl eframe::App for NClickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        
        // Register hotkeys on first frame if enabled
        if self.hotkeys_enabled && !self.hotkey_manager.is_registered() {
            self.register_hotkeys();
        }
        
        // Check for hotkey requests
        if self.clicker_state.check_and_clear_start_request() && !self.clicker_state.is_running() {
            self.start_clicking();
        }
        if self.clicker_state.check_and_clear_stop_request() && self.clicker_state.is_running() {
            self.stop_clicking();
        }
        
        // Show hotkey settings dialog
        if self.show_hotkey_dialog {
            egui::Window::new("Hotkey Settings")
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Configure Global Hotkeys");
                    ui.separator();
                    
                    ui.checkbox(&mut self.hotkeys_enabled, "Enable global hotkeys");
                    
                    ui.separator();
                    
                    // Start/Stop hotkey configuration
                    ui.horizontal(|ui| {
                        ui.label("Start/Stop:");
                        egui::ComboBox::from_id_source("start_modifier")
                            .selected_text(format!("{:?}", self.start_modifier))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.start_modifier, ModifierKey::None, "None");
                                ui.selectable_value(&mut self.start_modifier, ModifierKey::Alt, "Alt");
                                ui.selectable_value(&mut self.start_modifier, ModifierKey::Ctrl, "Ctrl");
                                ui.selectable_value(&mut self.start_modifier, ModifierKey::Shift, "Shift");
                                ui.selectable_value(&mut self.start_modifier, ModifierKey::AltCtrl, "Alt+Ctrl");
                            });
                        
                        egui::ComboBox::from_id_source("start_key")
                            .selected_text(format!("{:?}", self.start_key))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.start_key, FunctionKey::F1, "F1");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F2, "F2");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F3, "F3");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F4, "F4");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F5, "F5");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F6, "F6");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F7, "F7");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F8, "F8");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F9, "F9");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F10, "F10");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F11, "F11");
                                ui.selectable_value(&mut self.start_key, FunctionKey::F12, "F12");
                            });
                    });
                    
                    // Stop only hotkey configuration
                    ui.horizontal(|ui| {
                        ui.label("Stop only:");
                        egui::ComboBox::from_id_source("stop_modifier")
                            .selected_text(format!("{:?}", self.stop_modifier))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.stop_modifier, ModifierKey::None, "None");
                                ui.selectable_value(&mut self.stop_modifier, ModifierKey::Alt, "Alt");
                                ui.selectable_value(&mut self.stop_modifier, ModifierKey::Ctrl, "Ctrl");
                                ui.selectable_value(&mut self.stop_modifier, ModifierKey::Shift, "Shift");
                                ui.selectable_value(&mut self.stop_modifier, ModifierKey::AltCtrl, "Alt+Ctrl");
                            });
                        
                        egui::ComboBox::from_id_source("stop_key")
                            .selected_text(format!("{:?}", self.stop_key))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F1, "F1");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F2, "F2");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F3, "F3");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F4, "F4");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F5, "F5");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F6, "F6");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F7, "F7");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F8, "F8");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F9, "F9");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F10, "F10");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F11, "F11");
                                ui.selectable_value(&mut self.stop_key, FunctionKey::F12, "F12");
                            });
                    });
                    
                    ui.separator();
                    
                    ui.label(format!("Status: {}", self.hotkey_manager.get_status()));
                    
                    if self.clicker_state.is_hotkey_thread_running() {
                        ui.colored_label(egui::Color32::GREEN, "🔄 Hotkey listener thread: RUNNING");
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "⚠️ Hotkey listener thread: NOT RUNNING");
                    }
                    
                    if !self.hotkeys_enabled {
                        ui.colored_label(egui::Color32::YELLOW, "⚠️ Global hotkeys are disabled");
                    }
                    
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            self.unregister_hotkeys();
                            if self.hotkeys_enabled {
                                self.register_hotkeys();
                            }
                        }
                        
                        if ui.button("OK").clicked() {
                            self.unregister_hotkeys();
                            if self.hotkeys_enabled {
                                self.register_hotkeys();
                            }
                            self.show_hotkey_dialog = false;
                        }
                        
                        if ui.button("Cancel").clicked() {
                            self.show_hotkey_dialog = false;
                        }
                    });
                    
                    ui.separator();
                    ui.label("💡 Tip: If hotkeys don't work, try:");
                    ui.label("• Different keys (F9/F10 instead of F6/F7)");
                    ui.label("• Running as Administrator");
                    ui.label("• Closing other applications using those keys");
                });
        }
        
        egui::CentralPanel::default().show(ctx, |ui| {
            let title = if self.clicker_state.is_running() {
                "Running - nclicker Auto Clicker"
            } else {
                "Stopped - nclicker Auto Clicker"
            };
            ui.heading(title);
            ui.separator();
            
            // Click interval section
            ui.group(|ui| {
                ui.label("Click interval");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.hours).suffix(" hours").range(0..=23));
                    ui.add(egui::DragValue::new(&mut self.minutes).suffix(" mins").range(0..=59));
                    ui.add(egui::DragValue::new(&mut self.seconds).suffix(" secs").range(0..=59));
                    ui.add(egui::DragValue::new(&mut self.milliseconds).suffix(" milliseconds").range(0..=999));
                });
                
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.random_offset, "Random offset");
                    if self.random_offset {
                        ui.add(egui::DragValue::new(&mut self.random_offset_ms).suffix(" milliseconds").range(0..=10000));
                    }
                });
            });
            
            ui.separator();
            
            // Click options and repeat section
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.label("Click options");
                    ui.horizontal(|ui| {
                        ui.label("Mouse button:");
                        egui::ComboBox::from_id_source("mouse_button")
                            .selected_text(match self.mouse_button {
                                MouseButton::Left => "Left",
                                MouseButton::Right => "Right",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.mouse_button, MouseButton::Left, "Left");
                                ui.selectable_value(&mut self.mouse_button, MouseButton::Right, "Right");
                            });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Click type:");
                        egui::ComboBox::from_id_source("click_type")
                            .selected_text(&self.click_type)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.click_type, "Single".to_string(), "Single");
                                ui.selectable_value(&mut self.click_type, "Double".to_string(), "Double");
                            });
                    });
                });
                
                ui.group(|ui| {
                    ui.label("Click repeat");
                    ui.horizontal(|ui| {
                        if ui.radio_value(&mut self.click_mode, ClickMode::RepeatCount(self.repeat_count), "Repeat").clicked() {
                            self.click_mode = ClickMode::RepeatCount(self.repeat_count);
                        }
                        if matches!(self.click_mode, ClickMode::RepeatCount(_)) {
                            ui.add(egui::DragValue::new(&mut self.repeat_count).suffix(" times").range(1..=999999));
                            self.click_mode = ClickMode::RepeatCount(self.repeat_count);
                        }
                    });
                    ui.radio_value(&mut self.click_mode, ClickMode::RepeatUntilStopped, "Repeat until stopped");
                });
            });
            
            ui.separator();
            
            // Cursor position section
            ui.group(|ui| {
                ui.label("Cursor position");
                ui.radio_value(&mut self.use_current_position, true, "Current location");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.use_current_position, false, "Pick location");
                    if !self.use_current_position {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut self.cursor_x).range(0..=9999));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut self.cursor_y).range(0..=9999));
                    }
                });
            });
            
            ui.separator();
            
            // Theme selection
            ui.horizontal(|ui| {
                ui.label("Theme:");
                ui.radio_value(&mut self.current_theme, Theme::Light, "Light");
                ui.radio_value(&mut self.current_theme, Theme::Dark, "Dark");
            });
            
            ui.separator();
            
            // Control buttons
            ui.horizontal(|ui| {
                let start_text = format!("Start ({})", self.get_start_hotkey_string());
                let stop_text = format!("Stop ({})", self.get_stop_hotkey_string());
                
                if ui.button(&start_text).clicked() && !self.clicker_state.is_running() {
                    self.start_clicking();
                }
                
                if ui.button(&stop_text).clicked() && self.clicker_state.is_running() {
                    self.stop_clicking();
                }
                
                if ui.button("Hotkey setting").clicked() {
                    self.show_hotkey_dialog = true;
                }
                
                if ui.button("Record & Playback").clicked() {
                    // Placeholder for record and playback feature
                }
            });
            
            ui.separator();
            
            // Status information
            ui.horizontal(|ui| {
                if self.clicker_state.is_running() {
                    ui.colored_label(egui::Color32::GREEN, "● Status: RUNNING");
                } else {
                    ui.colored_label(egui::Color32::RED, "● Status: STOPPED");
                }
                ui.label(format!("Clicks: {}", self.clicker_state.get_click_count()));
                ui.label(format!("Interval: {}ms", self.calculate_interval_ms()));
            });
            
            // Hotkey status display with more detailed info
            if self.hotkeys_enabled && self.hotkey_manager.is_registered() {
                if self.clicker_state.is_hotkey_thread_running() {
                    ui.colored_label(egui::Color32::GREEN, 
                        format!("🎯 Global Hotkeys ACTIVE: {} (Start/Stop) | {} (Stop)", 
                            self.get_start_hotkey_string(), 
                            self.get_stop_hotkey_string()));
                } else {
                    ui.colored_label(egui::Color32::YELLOW, 
                        format!("⚠️ Hotkeys registered but listener not running: {} / {}", 
                            self.get_start_hotkey_string(), 
                            self.get_stop_hotkey_string()));
                }
            } else if self.hotkeys_enabled {
                ui.colored_label(egui::Color32::YELLOW, "⚠️ Hotkeys enabled but not registered - check settings");
            } else {
                ui.colored_label(egui::Color32::GRAY, "➤ Global hotkeys disabled - using buttons only");
            }
        });
    }
}

impl Drop for NClickerApp {
    fn drop(&mut self) {
        self.unregister_hotkeys();
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([450.0, 730.0])
            .with_resizable(true)
            .with_min_inner_size([400.0, 680.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "nclicker",
        options,
        Box::new(|_cc| Ok(Box::new(NClickerApp::default()))),
    )
}
