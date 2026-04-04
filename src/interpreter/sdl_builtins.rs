/// SDL2 built-in functions for the Legible language.
///
/// Provides window creation, rendering, input handling, and timing
/// through thread-local SDL2 state.
use std::cell::RefCell;
use std::collections::HashSet;

use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::EventPump;
use sdl2::Sdl;
use sdl2::TimerSubsystem;

use crate::errors::{LegibleError, ErrorCode, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

/// Thread-local SDL2 state.
struct SdlState {
    _context: Sdl,
    canvas: Canvas<Window>,
    event_pump: EventPump,
    timer: TimerSubsystem,
    pressed_keys: HashSet<String>,
    /// Path to the currently loaded font file.
    font_path: Option<String>,
    /// Point size of the currently loaded font.
    font_size: u16,
}

thread_local! {
    static SDL_STATE: RefCell<Option<SdlState>> = const { RefCell::new(None) };
}

fn sdl_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register all SDL2 built-in functions in the given environment.
pub fn register_sdl_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("sdl_init", builtin_sdl_init),
        ("sdl_poll_events", builtin_sdl_poll_events),
        ("sdl_is_key_pressed", builtin_sdl_is_key_pressed),
        ("sdl_clear", builtin_sdl_clear),
        ("sdl_fill_rect", builtin_sdl_fill_rect),
        ("sdl_present", builtin_sdl_present),
        ("sdl_delay", builtin_sdl_delay),
        ("sdl_get_ticks", builtin_sdl_get_ticks),
        ("sdl_quit", builtin_sdl_quit),
        ("sdl_load_font", builtin_sdl_load_font),
        ("sdl_draw_text", builtin_sdl_draw_text),
    ];

    for (name, func) in builtins {
        env.borrow_mut().define(
            name.to_string(),
            Value::Function(Callable::Builtin {
                name: name.to_string(),
                func,
            }),
            false,
        );
    }
}

fn require_integer(val: &Value, name: &str) -> Result<i64, LegibleError> {
    match val {
        Value::Integer(n) => Ok(*n),
        _ => Err(sdl_error(
            &format!("Expected integer for {name}, got {val}"),
            &format!("Pass an integer value for {name}"),
        )),
    }
}

fn require_text(val: &Value, name: &str) -> Result<String, LegibleError> {
    match val {
        Value::Text(s) => Ok(s.clone()),
        _ => Err(sdl_error(
            &format!("Expected text for {name}, got {val}"),
            &format!("Pass a text value for {name}"),
        )),
    }
}

/// Convert an SDL scancode to its display name.
fn scancode_name(scancode: Scancode) -> String {
    format!("{scancode:?}")
}

fn with_sdl<F, R>(f: F) -> Result<R, LegibleError>
where
    F: FnOnce(&mut SdlState) -> Result<R, LegibleError>,
{
    SDL_STATE.with(|state| {
        let mut borrow = state.borrow_mut();
        let sdl = borrow.as_mut().ok_or_else(|| {
            sdl_error(
                "SDL2 not initialized",
                "Call sdl_init(title, width, height) before using other SDL functions",
            )
        })?;
        f(sdl)
    })
}

/// `sdl_init(title: text, width: integer, height: integer): nothing`
fn builtin_sdl_init(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 3 {
        return Err(sdl_error(
            "sdl_init() expects 3 arguments",
            "Usage: sdl_init(title, width, height)",
        ));
    }
    let title = require_text(&args[0], "title")?;
    let width = require_integer(&args[1], "width")? as u32;
    let height = require_integer(&args[2], "height")? as u32;

    let context = sdl2::init().map_err(|e| sdl_error(&format!("SDL init failed: {e}"), "Check SDL2 installation"))?;
    let video = context
        .video()
        .map_err(|e| sdl_error(&format!("SDL video init failed: {e}"), "Check display availability"))?;
    let window = video
        .window(&title, width, height)
        .position_centered()
        .build()
        .map_err(|e| sdl_error(&format!("Window creation failed: {e}"), "Check window parameters"))?;
    let canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .map_err(|e| sdl_error(&format!("Canvas creation failed: {e}"), "Check renderer availability"))?;
    let event_pump = context
        .event_pump()
        .map_err(|e| sdl_error(&format!("Event pump failed: {e}"), "Check SDL2 initialization"))?;
    let timer = context
        .timer()
        .map_err(|e| sdl_error(&format!("Timer init failed: {e}"), "Check SDL2 initialization"))?;

    SDL_STATE.with(|state| {
        *state.borrow_mut() = Some(SdlState {
            _context: context,
            canvas,
            event_pump,
            timer,
            pressed_keys: HashSet::new(),
            font_path: None,
            font_size: 16,
        });
    });

    Ok(Value::None)
}

/// `sdl_poll_events(): a list of text`
fn builtin_sdl_poll_events(_args: &[Value]) -> Result<Value, LegibleError> {
    with_sdl(|sdl| {
        let mut events = Vec::new();
        let collected: Vec<Event> = sdl.event_pump.poll_iter().collect();
        for event in collected {
            match event {
                Event::Quit { .. } => {
                    events.push(Value::Text("quit".to_string()));
                }
                Event::KeyDown {
                    scancode: Some(sc),
                    repeat: false,
                    ..
                } => {
                    let name = scancode_name(sc);
                    sdl.pressed_keys.insert(name.clone());
                    events.push(Value::Text(format!("key_down:{name}")));
                }
                Event::KeyUp {
                    scancode: Some(sc), ..
                } => {
                    let name = scancode_name(sc);
                    sdl.pressed_keys.remove(&name);
                    events.push(Value::Text(format!("key_up:{name}")));
                }
                _ => {}
            }
        }
        Ok(Value::List(events))
    })
}

/// `sdl_is_key_pressed(key: text): boolean`
fn builtin_sdl_is_key_pressed(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(sdl_error(
            "sdl_is_key_pressed() expects 1 argument",
            "Usage: sdl_is_key_pressed(key_name)",
        ));
    }
    let key = require_text(&args[0], "key")?;
    with_sdl(|sdl| Ok(Value::Boolean(sdl.pressed_keys.contains(&key))))
}

/// `sdl_clear(r: integer, g: integer, b: integer): nothing`
fn builtin_sdl_clear(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 3 {
        return Err(sdl_error(
            "sdl_clear() expects 3 arguments",
            "Usage: sdl_clear(r, g, b)",
        ));
    }
    let r = require_integer(&args[0], "r")? as u8;
    let g = require_integer(&args[1], "g")? as u8;
    let b = require_integer(&args[2], "b")? as u8;

    with_sdl(|sdl| {
        sdl.canvas.set_draw_color(Color::RGB(r, g, b));
        sdl.canvas.clear();
        Ok(Value::None)
    })
}

/// `sdl_fill_rect(x: integer, y: integer, w: integer, h: integer, r: integer, g: integer, b: integer): nothing`
fn builtin_sdl_fill_rect(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 7 {
        return Err(sdl_error(
            "sdl_fill_rect() expects 7 arguments",
            "Usage: sdl_fill_rect(x, y, w, h, r, g, b)",
        ));
    }
    let x = require_integer(&args[0], "x")? as i32;
    let y = require_integer(&args[1], "y")? as i32;
    let w = require_integer(&args[2], "w")? as u32;
    let h = require_integer(&args[3], "h")? as u32;
    let r = require_integer(&args[4], "r")? as u8;
    let g = require_integer(&args[5], "g")? as u8;
    let b = require_integer(&args[6], "b")? as u8;

    with_sdl(|sdl| {
        sdl.canvas.set_draw_color(Color::RGB(r, g, b));
        sdl.canvas
            .fill_rect(Rect::new(x, y, w, h))
            .map_err(|e| sdl_error(&format!("fill_rect failed: {e}"), "Check rectangle parameters"))?;
        Ok(Value::None)
    })
}

/// `sdl_present(): nothing`
fn builtin_sdl_present(_args: &[Value]) -> Result<Value, LegibleError> {
    with_sdl(|sdl| {
        sdl.canvas.present();
        Ok(Value::None)
    })
}

/// `sdl_delay(ms: integer): nothing`
fn builtin_sdl_delay(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(sdl_error(
            "sdl_delay() expects 1 argument",
            "Usage: sdl_delay(milliseconds)",
        ));
    }
    let ms = require_integer(&args[0], "ms")? as u32;
    std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
    Ok(Value::None)
}

/// `sdl_get_ticks(): integer`
fn builtin_sdl_get_ticks(_args: &[Value]) -> Result<Value, LegibleError> {
    with_sdl(|sdl| Ok(Value::Integer(i64::from(sdl.timer.ticks()))))
}

/// `sdl_quit(): nothing`
fn builtin_sdl_quit(_args: &[Value]) -> Result<Value, LegibleError> {
    SDL_STATE.with(|state| {
        *state.borrow_mut() = None;
    });
    Ok(Value::None)
}

/// `sdl_load_font(path: text, size: integer): nothing`
///
/// Stores the font path and size for subsequent `sdl_draw_text` calls.
/// The font file is loaded from disk on each draw call.
fn builtin_sdl_load_font(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(sdl_error(
            "sdl_load_font() expects 2 arguments",
            "Usage: sdl_load_font(path, size) — e.g. sdl_load_font(\"/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf\", 24)",
        ));
    }
    let path = require_text(&args[0], "path")?;
    let size = require_integer(&args[1], "size")?;
    if size < 1 || size > 512 {
        return Err(sdl_error(
            "Font size must be between 1 and 512",
            "Use a reasonable point size such as 16 or 24",
        ));
    }
    with_sdl(|sdl| {
        sdl.font_path = Some(path);
        sdl.font_size = size as u16;
        Ok(Value::None)
    })
}

/// `sdl_draw_text(text: text, x: integer, y: integer, r: integer, g: integer, b: integer): nothing`
///
/// Renders `text` at pixel position `(x, y)` using the font loaded by `sdl_load_font`.
/// Color is specified as RGB integers 0–255.
fn builtin_sdl_draw_text(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 6 {
        return Err(sdl_error(
            "sdl_draw_text() expects 6 arguments",
            "Usage: sdl_draw_text(text, x, y, r, g, b)",
        ));
    }
    let text = require_text(&args[0], "text")?;
    let x = require_integer(&args[1], "x")? as i32;
    let y = require_integer(&args[2], "y")? as i32;
    let r = require_integer(&args[3], "r")? as u8;
    let g = require_integer(&args[4], "g")? as u8;
    let b = require_integer(&args[5], "b")? as u8;

    with_sdl(|sdl| {
        let (font_path, font_size) = match &sdl.font_path {
            Some(p) => (p.clone(), sdl.font_size),
            None => {
                return Err(sdl_error(
                    "No font loaded",
                    "Call sdl_load_font(path, size) before sdl_draw_text",
                ));
            }
        };

        let ttf_context = sdl2::ttf::init()
            .map_err(|e| sdl_error(&format!("TTF init failed: {e}"), "Check SDL2_ttf installation"))?;

        let font = ttf_context
            .load_font(&font_path, font_size)
            .map_err(|e| sdl_error(&format!("Font load failed: {e}"), "Check that the font path exists and is a valid TTF/OTF file"))?;

        let surface = font
            .render(&text)
            .blended(sdl2::pixels::Color::RGB(r, g, b))
            .map_err(|e| sdl_error(&format!("Text render failed: {e}"), "Check font and text parameters"))?;

        let texture_creator = sdl.canvas.texture_creator();
        let texture = texture_creator
            .create_texture_from_surface(&surface)
            .map_err(|e| sdl_error(&format!("Texture creation failed: {e}"), "Check available GPU memory"))?;

        let sdl2::render::TextureQuery { width, height, .. } = texture.query();
        let dst = sdl2::rect::Rect::new(x, y, width, height);

        sdl.canvas
            .copy(&texture, None, dst)
            .map_err(|e| sdl_error(&format!("Canvas copy failed: {e}"), "Check canvas state"))?;

        Ok(Value::None)
    })
}
