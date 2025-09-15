use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, VK_F1, VK_F2, VK_F3, VK_F4,
    VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12,
    GetAsyncKeyState, VK_MENU, VK_CONTROL, VK_SHIFT
};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
use windows::Win32::System::Registry::{RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, HKEY};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::core::HSTRING;
use std::ptr;

const HOTKEY_POLL_INTERVAL_MS: u64 = 50; // Increased to 50ms for more reliable detection

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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Theme {
    SystemDefault,
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
    fn is_pressed(&self) -> bool {
        unsafe {
            match self {
                ModifierKey::None => true, // No modifier required
                ModifierKey::Alt => (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000u16) != 0,
                ModifierKey::Ctrl => (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000u16) != 0,
                ModifierKey::Shift => (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000u16) != 0,
                ModifierKey::AltCtrl => {
                    let alt_pressed = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000u16) != 0;
                    let ctrl_pressed = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000u16) != 0;
                    alt_pressed && ctrl_pressed
                }
            }
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
    fn is_pressed(&self) -> bool {
        unsafe {
            let vk_code = match self {
                FunctionKey::F1 => VK_F1.0,
                FunctionKey::F2 => VK_F2.0,
                FunctionKey::F3 => VK_F3.0,
                FunctionKey::F4 => VK_F4.0,
                FunctionKey::F5 => VK_F5.0,
                FunctionKey::F6 => VK_F6.0,
                FunctionKey::F7 => VK_F7.0,
                FunctionKey::F8 => VK_F8.0,
                FunctionKey::F9 => VK_F9.0,
                FunctionKey::F10 => VK_F10.0,
                FunctionKey::F11 => VK_F11.0,
                FunctionKey::F12 => VK_F12.0,
            };
            (GetAsyncKeyState(vk_code as i32) as u16 & 0x8000u16) != 0
        }
    }
    
    fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

// Function to detect Windows dark mode
fn is_windows_dark_mode() -> bool {
    unsafe {
        let key_name = HSTRING::from("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let mut hkey: HKEY = HKEY(ptr::null_mut());
        
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &key_name,
            0,
            KEY_READ,
            &mut hkey
        );
        
        if result != ERROR_SUCCESS {
            return false; // Default to light mode if can't read
        }
        
        let value_name = HSTRING::from("AppsUseLightTheme");
        let mut data: u32 = 0;
        let mut data_size = std::mem::size_of::<u32>() as u32;
        
        let result = RegQueryValueExW(
            hkey,
            &value_name,
            Some(ptr::null()),
            None,
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut data_size)
        );
        
        if result == ERROR_SUCCESS {
            // 0 means dark mode, 1 means light mode
            data == 0
        } else {
            false // Default to light mode
        }
    }
}

#[derive(Clone)]
struct ClickingConfig {
    interval_ms: u64,
    mouse_button: MouseButton,
    click_type: String,
    click_mode: ClickMode,
    use_current_position: bool,
    cursor_x: i32,
    cursor_y: i32,
    random_offset: bool,
    random_offset_ms: u32,
}

#[derive(Clone)]
struct ClickerState {
    is_running: Arc<Mutex<bool>>,
    click_count: Arc<Mutex<u32>>,
    should_start: Arc<Mutex<bool>>,
    should_stop: Arc<Mutex<bool>>,
    hotkey_thread_running: Arc<Mutex<bool>>,
    clicking_config: Arc<Mutex<Option<ClickingConfig>>>,
}

impl ClickerState {
    fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            click_count: Arc::new(Mutex::new(0)),
            should_start: Arc::new(Mutex::new(false)),
            should_stop: Arc::new(Mutex::new(false)),
            hotkey_thread_running: Arc::new(Mutex::new(false)),
            clicking_config: Arc::new(Mutex::new(None)),
        }
    }
    
    fn start_clicking_with_config(&self, config: ClickingConfig) {
        if *self.is_running.lock().unwrap() {
            return; // Already running
        }
        
        *self.is_running.lock().unwrap() = true;
        *self.click_count.lock().unwrap() = 0;
        *self.clicking_config.lock().unwrap() = Some(config.clone());
        
        println!("Starting clicking with config!"); // Debug
        
        let clicker_state = self.clone();
        
        thread::spawn(move || {
            let mut clicks_performed = 0;
            
            while *clicker_state.is_running.lock().unwrap() {
                // Check if we should stop based on repeat count
                if let ClickMode::RepeatCount(max_clicks) = config.click_mode {
                    if clicks_performed >= max_clicks {
                        break;
                    }
                }
                
                // Set cursor position if needed
                unsafe {
                    if !config.use_current_position {
                        let _ = SetCursorPos(config.cursor_x, config.cursor_y);
                        thread::sleep(Duration::from_millis(10));
                    }
                    
                    // Perform click
                    match config.mouse_button {
                        MouseButton::Left => {
                            let _ = mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                            let _ = mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                            
                            if config.click_type == "Double" {
                                thread::sleep(Duration::from_millis(10));
                                let _ = mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                                let _ = mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                            }
                        }
                        MouseButton::Right => {
                            let _ = mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
                            let _ = mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
                            
                            if config.click_type == "Double" {
                                thread::sleep(Duration::from_millis(10));
                                let _ = mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
                                let _ = mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
                            }
                        }
                    }
                }
                
                clicks_performed += 1;
                *clicker_state.click_count.lock().unwrap() += 1;
                
                // Calculate sleep duration with optional random offset
                let mut sleep_duration = config.interval_ms;
                if config.random_offset && config.random_offset_ms > 0 {
                    let offset = fastrand::u32(0..=config.random_offset_ms);
                    sleep_duration = sleep_duration.saturating_add(offset as u64);
                }
                
                thread::sleep(Duration::from_millis(sleep_duration));
            }
            
            *clicker_state.is_running.lock().unwrap() = false;
            println!("Clicking thread stopped!"); // Debug
        });
    }
    
    fn stop_clicking(&self) {
        *self.is_running.lock().unwrap() = false;
        println!("Requested clicking stop!"); // Debug
    }
    
    fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }
    
    fn get_click_count(&self) -> u32 {
        *self.click_count.lock().unwrap()
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

#[derive(Clone)]
struct GlobalHotkeyThread {
    should_stop: Arc<Mutex<bool>>,
    is_running: Arc<Mutex<bool>>,
}

impl GlobalHotkeyThread {
    fn new() -> Self {
        Self {
            should_stop: Arc::new(Mutex::new(false)),
            is_running: Arc::new(Mutex::new(false)),
        }
    }
    
    fn start(&self, start_mod: ModifierKey, start_key: FunctionKey, stop_mod: ModifierKey, stop_key: FunctionKey, clicker_state: ClickerState, clicking_config: ClickingConfig) {
        *self.should_stop.lock().unwrap() = false;
        *self.is_running.lock().unwrap() = true;
        
        let should_stop = self.should_stop.clone();
        let is_running = self.is_running.clone();
        let clicker_state_for_thread = clicker_state.clone();
        
        thread::spawn(move || {
            println!("Global hotkey thread started!"); // Debug
            
            let mut f6_was_pressed = false;
            let mut f7_was_pressed = false;
            let mut last_action_time = Instant::now() - Duration::from_secs(1);
            
            while !*should_stop.lock().unwrap() {
                let now = Instant::now();
                let debounce_time = Duration::from_millis(300);
                
                // Check start/stop hotkey (F6 by default)
                let start_pressed = start_mod.is_pressed() && start_key.is_pressed();
                if start_pressed && !f6_was_pressed && now.duration_since(last_action_time) > debounce_time {
                    println!("F6 pressed! Current state: {}", clicker_state_for_thread.is_running()); // Debug
                    if clicker_state_for_thread.is_running() {
                        // Stop clicking directly
                        clicker_state_for_thread.stop_clicking();
                        println!("STOPPED clicking via hotkey"); // Debug
                    } else {
                        // Start clicking directly
                        clicker_state_for_thread.start_clicking_with_config(clicking_config.clone());
                        println!("STARTED clicking via hotkey"); // Debug
                    }
                    last_action_time = now;
                }
                f6_was_pressed = start_pressed;
                
                // Check stop-only hotkey (F7 by default) - only if different from start key
                if start_key != stop_key || start_mod != stop_mod {
                    let stop_pressed = stop_mod.is_pressed() && stop_key.is_pressed();
                    if stop_pressed && !f7_was_pressed && now.duration_since(last_action_time) > debounce_time {
                        println!("F7 pressed! Stopping via hotkey"); // Debug
                        clicker_state_for_thread.stop_clicking();
                        last_action_time = now;
                    }
                    f7_was_pressed = stop_pressed;
                }
                
                thread::sleep(Duration::from_millis(HOTKEY_POLL_INTERVAL_MS));
            }
            
            *is_running.lock().unwrap() = false;
            println!("Global hotkey thread stopped!"); // Debug
        });
        
        clicker_state.set_hotkey_thread_running(true);
    }
    
    fn stop(&self) {
        *self.should_stop.lock().unwrap() = true;
        // Wait a bit for thread to stop
        for _ in 0..10 {
            if !*self.is_running.lock().unwrap() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    
    fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }
}

struct HotkeyManager {
    enabled: bool,
    status: String,
    hotkey_thread: Option<GlobalHotkeyThread>,
}

impl HotkeyManager {
    fn new() -> Self {
        Self {
            enabled: false,
            status: "Ready to start global hotkey polling".to_string(),
            hotkey_thread: None,
        }
    }
    
    fn start_polling(&mut self, start_mod: ModifierKey, start_key: FunctionKey, stop_mod: ModifierKey, stop_key: FunctionKey, clicker_state: ClickerState, clicking_config: ClickingConfig) {
        // Stop any existing thread
        if let Some(ref thread) = self.hotkey_thread {
            thread.stop();
        }
        
        // Create and start new thread
        let thread = GlobalHotkeyThread::new();
        thread.start(start_mod, start_key, stop_mod, stop_key, clicker_state, clicking_config);
        
        self.hotkey_thread = Some(thread);
        self.enabled = true;
        self.status = format!("✅ Global hotkeys active: {}{} (Start/Stop) | {}{} (Stop)",
            start_mod.to_string(), start_key.to_string(),
            stop_mod.to_string(), stop_key.to_string());
        
        println!("Hotkey manager started polling"); // Debug
    }
    
    fn stop_polling(&mut self) {
        if let Some(ref thread) = self.hotkey_thread {
            thread.stop();
        }
        self.hotkey_thread = None;
        self.enabled = false;
        self.status = "Global hotkey polling stopped".to_string();
        println!("Hotkey manager stopped polling"); // Debug
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    fn is_thread_running(&self) -> bool {
        if let Some(ref thread) = self.hotkey_thread {
            thread.is_running()
        } else {
            false
        }
    }
    
    fn get_status(&self) -> &str {
        &self.status
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.stop_polling();
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
            current_theme: Theme::SystemDefault, // Default to system theme
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
    
    fn get_clicking_config(&self) -> ClickingConfig {
        ClickingConfig {
            interval_ms: self.calculate_interval_ms(),
            mouse_button: self.mouse_button,
            click_type: self.click_type.clone(),
            click_mode: self.click_mode,
            use_current_position: self.use_current_position,
            cursor_x: self.cursor_x,
            cursor_y: self.cursor_y,
            random_offset: self.random_offset,
            random_offset_ms: self.random_offset_ms,
        }
    }
    
    fn start_hotkey_polling(&mut self) {
        if !self.hotkeys_enabled {
            return;
        }
        
        self.hotkey_manager.start_polling(
            self.start_modifier, 
            self.start_key, 
            self.stop_modifier, 
            self.stop_key, 
            self.clicker_state.clone(),
            self.get_clicking_config()
        );
    }
    
    fn stop_hotkey_polling(&mut self) {
        self.hotkey_manager.stop_polling();
        self.clicker_state.set_hotkey_thread_running(false);
    }
    
    fn start_clicking(&mut self) {
        if self.clicker_state.is_running() {
            return;
        }
        
        let config = self.get_clicking_config();
        self.clicker_state.start_clicking_with_config(config);
    }
    
    fn stop_clicking(&mut self) {
        self.clicker_state.stop_clicking();
    }
    
    fn apply_theme(&self, ctx: &egui::Context) {
        match self.current_theme {
            Theme::SystemDefault => {
                if is_windows_dark_mode() {
                    ctx.set_visuals(egui::Visuals::dark());
                } else {
                    ctx.set_visuals(egui::Visuals::light());
                }
            },
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
        // Force regular UI updates even when not focused
        ctx.request_repaint_after(Duration::from_millis(100));
        
        self.apply_theme(ctx);
        
        // Start hotkey polling on first frame if enabled
        if self.hotkeys_enabled && !self.hotkey_manager.is_enabled() {
            self.start_hotkey_polling();
        }
        
        // Check for hotkey requests (though now they're handled directly)
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
                    
                    if self.hotkey_manager.is_thread_running() {
                        ui.colored_label(egui::Color32::GREEN, "🔄 Global hotkey thread: RUNNING");
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "⚠️ Global hotkey thread: STOPPED");
                    }
                    
                    if !self.hotkeys_enabled {
                        ui.colored_label(egui::Color32::YELLOW, "⚠️ Global hotkeys are disabled");
                    }
                    
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            self.stop_hotkey_polling();
                            if self.hotkeys_enabled {
                                self.start_hotkey_polling();
                            }
                        }
                        
                        if ui.button("OK").clicked() {
                            self.stop_hotkey_polling();
                            if self.hotkeys_enabled {
                                self.start_hotkey_polling();
                            }
                            self.show_hotkey_dialog = false;
                        }
                        
                        if ui.button("Cancel").clicked() {
                            self.show_hotkey_dialog = false;
                        }
                    });
                    
                    ui.separator();
                    ui.label("💡 Global hotkeys work even when app is not focused");
                    ui.label("Check console output for debugging info");
                    ui.label("Try pressing F6 while this app is in background");
                });
        }
        
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0; // Reduce vertical spacing
            ui.spacing_mut().indent = 8.0; // Reduce indentation
            
            let title = if self.clicker_state.is_running() {
                "Running - nclicker Auto Clicker"
            } else {
                "Stopped - nclicker Auto Clicker"
            };
            ui.heading(title);
            ui.add_space(4.0);
            
            // Very compact layout - everything tightly packed
            ui.horizontal(|ui| {
                // Click interval section (left side)
                ui.group(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label("Click interval");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.hours).suffix("h").range(0..=23).speed(0.1));
                        ui.add(egui::DragValue::new(&mut self.minutes).suffix("m").range(0..=59).speed(0.1));
                        ui.add(egui::DragValue::new(&mut self.seconds).suffix("s").range(0..=59).speed(0.1));
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.milliseconds).suffix("ms").range(0..=999).speed(1));
                        ui.checkbox(&mut self.random_offset, "±Rnd");
                    });
                    if self.random_offset {
                        ui.horizontal(|ui| {
                            ui.label("±");
                            ui.add(egui::DragValue::new(&mut self.random_offset_ms).suffix("ms").range(0..=10000).speed(10));
                        });
                    }
                });
                
                // Cursor position section (right side) 
                ui.group(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label("Cursor position");
                    ui.radio_value(&mut self.use_current_position, true, "Current");
                    ui.radio_value(&mut self.use_current_position, false, "Fixed");
                    if !self.use_current_position {
                        ui.horizontal(|ui| {
                            ui.label("X:");
                            ui.add(egui::DragValue::new(&mut self.cursor_x).range(0..=9999).speed(1));
                            ui.label("Y:");
                            ui.add(egui::DragValue::new(&mut self.cursor_y).range(0..=9999).speed(1));
                        });
                    }
                });
            });
            
            ui.add_space(4.0);
            
            // Click options and repeat in one compact row
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label("Click options");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_source("mouse_button")
                            .selected_text(match self.mouse_button {
                                MouseButton::Left => "Left",
                                MouseButton::Right => "Right",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.mouse_button, MouseButton::Left, "Left");
                                ui.selectable_value(&mut self.mouse_button, MouseButton::Right, "Right");
                            });
                        
                        egui::ComboBox::from_id_source("click_type")
                            .selected_text(&self.click_type)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.click_type, "Single".to_string(), "Single");
                                ui.selectable_value(&mut self.click_type, "Double".to_string(), "Double");
                            });
                    });
                });
                
                ui.group(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label("Click repeat");
                    ui.horizontal(|ui| {
                        if ui.radio_value(&mut self.click_mode, ClickMode::RepeatCount(self.repeat_count), "Count").clicked() {
                            self.click_mode = ClickMode::RepeatCount(self.repeat_count);
                        }
                        if matches!(self.click_mode, ClickMode::RepeatCount(_)) {
                            ui.add(egui::DragValue::new(&mut self.repeat_count).range(1..=999999).speed(1));
                            self.click_mode = ClickMode::RepeatCount(self.repeat_count);
                        }
                    });
                    ui.radio_value(&mut self.click_mode, ClickMode::RepeatUntilStopped, "Until stopped");
                });
            });
            
            ui.add_space(4.0);
            
            // Theme and control buttons in same row - very compact
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.current_theme, Theme::SystemDefault, "System");
                ui.radio_value(&mut self.current_theme, Theme::Light, "Light");
                ui.radio_value(&mut self.current_theme, Theme::Dark, "Dark");
                
                ui.separator();
                
                let start_text = format!("Start ({})", self.get_start_hotkey_string());
                let stop_text = format!("Stop ({})", self.get_stop_hotkey_string());
                
                if ui.button(&start_text).clicked() && !self.clicker_state.is_running() {
                    self.start_clicking();
                }
                
                if ui.button(&stop_text).clicked() && self.clicker_state.is_running() {
                    self.stop_clicking();
                }
                
                if ui.button("Hotkeys").clicked() {
                    self.show_hotkey_dialog = true;
                }
            });
            
            ui.add_space(4.0);
            ui.separator();
            
            // Status information - very compact
            ui.horizontal(|ui| {
                if self.clicker_state.is_running() {
                    ui.colored_label(egui::Color32::GREEN, "● RUNNING");
                } else {
                    ui.colored_label(egui::Color32::RED, "● STOPPED");
                }
                ui.label(format!("Clicks: {}", self.clicker_state.get_click_count()));
                ui.label(format!("Interval: {}ms", self.calculate_interval_ms()));
            });
            
            // Hotkey status display - compact single line
            if self.hotkeys_enabled && self.hotkey_manager.is_enabled() && self.hotkey_manager.is_thread_running() {
                ui.colored_label(egui::Color32::GREEN, 
                    format!("🎯 Global Hotkeys ACTIVE: {} (Start/Stop) | {} (Stop)", 
                        self.get_start_hotkey_string(), 
                        self.get_stop_hotkey_string()));
            } else if self.hotkeys_enabled {
                ui.colored_label(egui::Color32::YELLOW, "⚠️ Hotkeys enabled but thread not running");
            } else {
                ui.colored_label(egui::Color32::GRAY, "➤ Global hotkeys disabled");
            }
        });
    }
}

impl Drop for NClickerApp {
    fn drop(&mut self) {
        self.stop_hotkey_polling();
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 320.0])  // Taller and slightly wider to fit everything
            .with_resizable(false)            // Non-resizable
            .with_min_inner_size([560.0, 320.0])
            .with_max_inner_size([560.0, 320.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "nclicker",
        options,
        Box::new(|_cc| Ok(Box::new(NClickerApp::default()))),
    )
}
