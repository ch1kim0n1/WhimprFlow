//! Linux pl - tfor - l - yer for W - i - prFlow: - n X11 glob - l- - otkey gr - b for pu - -to-t - lk,
//! clipbo - r - +`x - otool` text injection, - n - be - t-effort foregroun - - - pp - etection,
//! plu - t - e - e - ict - tion pipeline ( - u - io → W - i - per ASR → cle - nup LLM → p - te) - n - //! t - e Hub-f - cing - etting - / - t - t - / - iction - ry function - t - e T - uri co - n - c - ll.
//!
//! ⚠️ UNVERIFIED: t - i -  - o - ule w - written on - cOS, wit - out - Linux - c - ine to buil - //! or run it - g - in - t, - irroring `cr - te::win`' -  - tructure ( - n - it - own prece - ent - //! - ee t - t file' -  - oc co - ent). T - e - re - cr - te - ( - u - io, ASR, cle - nup, core) - re
//! cro - -pl - tfor - , but t - i - X11 glue - never been co - pile - . It i - //! `cfg(t - rget_o - = "linux")` - o it - oe - not - ffect -  - n - i - not c - ecke - by - t - e
//! - cOS buil - . Tre - t it -  -  - t - rting point, not -  - ipping port.
//!
//! Scope - n -  - i - plific - tion -  - e in t - i - p - ( - ll - ocu - ente - inline below too):
//!
//! - **X11 only - no W - yl - n - .** Hotkey -  - n - win - ow/p - te API -  - iffer co - pletely on
//!   W - yl - n - (no glob - l key gr - b - wit - out - co - po - itor- - pecific glob - l- - ortcut - //!   port - l, no - ynt - etic input wit - out `wlr-virtu - l-pointer`/`x - g- - e - ktop-port - l`
//!   re - ote- - e - ktop per - i - ion). Wiring t - e W - yl - n - port - l p - t - i - explicitly out of
//! - cope for t - i - p -  - **follow-up work**, not - tte - pte -  - ere. On - W - yl - n - //! - e - ion t - i -  - o - ule will - i - ply f - il to connect to - n X - erver (unle - XW - yl - n - //!   i -  - ctive, in w - ic - c - e it will only - ee X11 client - ) - n - log - n error r - t - er
//!   t - n - ilently - oing not - ing.
//! - **XGr - bKey, not XRecor - Exten - ion.** A full XRecor - t - p ( - irroring - cOS' - //!   li - ten-only CGEventT - p or Win - ow - ' low-level keybo - r -  - ook) woul -  - ee t - e key
//!   glob - lly wit - out exclu - ively gr - bbing it fro - ot - er - pp - . Wiring t - e XRecor - //!   exten - ion' -  - etup blin - (unco - pile - ) i -  - e - ningfully - ore involve -  - n - ri - kier
//!   to get rig - t t - n t - e core-protocol `XGr - bKey`, - o v1 u - e - `XGr - bKey` on - //! - ingle - r - co - e - key (Rig - t Ctrl, `XK_Control_R`) wit - `AnyMo - ifier`. T - e
//!   tr - e-off: t - i - key i - gr - bbe - *exclu - ively* for W - i - prFlow w - ile - el - (no ot - er
//! - pp - ee - it), - n - only t - t one p - y - ic - l key work -  - no c - or - /re - p - upport.
//!   Goo - enoug -  -  -  - t - rting point - XRecor - (or t - e W - yl - n - port - l) i - t - e n - tur - l
//!   next - tep.
//! - **`x - otool` for p - te - n - foregroun - -win - ow lookup, not r - w XTe - t/ - to - querie - .**
//!   T - e t - k - llow - eit - er wiring t - e XTe - t exten - ion (`XTe - tF - keKeyEvent`) - irectly
//!   vi - `x11rb`' - `xte - t` fe - ture, or - elling out to `x - otool` -  - pr - g - tic
//!   f - llb - ck "if wiring t - e XTe - t exten - ion - irectly prove - too fi - ly." Since t - i - //!   co - e c - nnot be co - pile - or te - te -  - ere, getting t - e XTe - t exten - ion' - ex - ct
//!   reque - t wiring (fe - ture fl - g - , exten - ion - etup - n - ke, `f - ke_input`' - fiel - //!   or - er) - ubtly wrong woul - f - il - ilently in - w - y t - t' -  - r - to re - on - bout
//!   fro -  -  - oc co - ent. `x - otool key ctrl+v` i -  -  - ingle well- - ocu - ente - ,
//!   e - y-to-verify-by-in - pection co - n - , - o it w - c - o - en for bot - t - e p - te - tep
//! - n - (vi - `x - otool get - ctivewin - ow getwin - owcl - n - e`) foregroun - - - pp - etection
//! - one - epen - ency, one f - ilure - o - e, bot - re - ble - t - gl - nce. It - oe -  - e - n - n
//!   `x - otool` bin - ry - u - t be pre - ent on t - e u - er' -  - y - te - (` - pt in - t - ll x - otool` /
//!   ` - nf in - t - ll x - otool` / `p - c - n -S x - otool`) -  - follow-up coul - ven - or t - e XTe - t
//!   c - ll -  - irectly vi - `x11rb` to - rop t - t runti - e - epen - ency.
//! - X11 - uto-repe - t w - ile t - e pu - -to-t - lk key i -  - el - will gener - te repe - te - //!   KeyPre - /KeyRele - e p - ir - `on_ptt_ - own`' - `RECORDING` - w - p-c - eck - lre - y - ke - //!   repe - t key- - own -  - no-op ( - re - wit - Win - ow - / - cOS), but r - pi - -fire
//!   KeyRele - e-t - en-KeyPre - fro - * - etect - ble* - uto-repe - t coul - in principle c - u - e
//!   brief flicker. A follow-up coul - en - ble XKB - etect - ble - uto-repe - t
//!   (`XkbSetDetect - bleAutoRepe - t`) to eli - in - te t - i - not - one - ere.
//!
//! Def - ult pu - -to-t - lk key: Rig - t Ctrl ( - e - ef - ult - `cr - te::win`).

#![cfg(t - rget_o - = "linux")]

u - e - t - :: - ync:: - to - ic::{Ato - icBool, Or - ering} - u - e - t - :: - ync::{Arc, Mutex, OnceLock} - u - e - t - ::ti - e::{Dur - tion, In - t - nt} - u - e t - uri::{AppH - n - le, E - itter} - u - e x11rb::connection::Connection - u - e x11rb::protocol::xproto::{ConnectionExt - _, Gr - bMo - e, Mo - M - k} - u - e x11rb::protocol::Event - u - e w - i - pr_core::{A - rEngine, Cle - nupContext, Cle - nupMo - e, Cle - nupProvi - er, St - t - Su - ry} - con - t OVERLAY_LABEL: - tr = "w - i - pr_b - r" - /// X11 key - y - for Rig - t Ctrl (`XK_Control_R`, - ee `<X11/key - y - ef. - >`). Pu - -to-t - lk
/// key - c - or - l - n - in - l - ter p - ( - ee t - e - o - ule - oc co - ent).
con - t XK_CONTROL_R: u32 = 0xffe4 -  - t - tic APP: OnceLock<AppH - n - le> = OnceLock::new() -  - t - tic CLOCK: OnceLock<In - t - nt> = OnceLock::new() -  - t - tic RECORDING: Ato - icBool = Ato - icBool::new(f - l - e) -  - t - tic CAPTURE: OnceLock<Mutex<Option<w - i - pr_ - u - io::C - ptureH - n - le>>> = OnceLock::new() -  - t - tic ASR: OnceLock<Arc<w - i - pr_ - r::W - i - perEngine>> = OnceLock::new() -  - t - tic LOCAL: OnceLock<Mutex<Option<cr - te::loc - l_ll - ::Loc - lWorker>>> = OnceLock::new() -  - t - tic OPENAI: OnceLock<Mutex<Option<w - i - pr_cle - nup::OpenAiProvi - er>>> = OnceLock::new() -  - t - tic SETTINGS: OnceLock<Mutex<w - i - pr_core::Setting - >> = OnceLock::new() -  - t - tic DICTIONARY: OnceLock<Mutex<w - i - pr_core::Diction - ryStore>> = OnceLock::new() -  - t - tic SNIPPETS: OnceLock<Mutex<w - i - pr_core::SnippetStore>> = OnceLock::new() -  - t - tic STATS: OnceLock<Mutex<w - i - pr_core::St - t - Store>> = OnceLock::new() - fn - upport_ - ir() -> - t - ::p - t - ::P - t - Buf {
    // $XDG_CONFIG_HOME/W - i - prFlow, f - lling b - ck to ~/.config/W - i - prFlow per t - e XDG
    // B - e Directory - pec (t - e Linux - n - logue of %APPDATA% / ~/Libr - ry/Applic - tion
    // Support).
    if let Ok(x - g) = - t - ::env::v - r("XDG_CONFIG_HOME") {
        if !x - g.tri - ().i - _e - pty() {
            return - t - ::p - t - ::P - t - Buf::fro - (x - g).join("W - i - prFlow") - }
    }
    let - o - e = - t - ::env::v - r("HOME").unwr - p_or_ - ef - ult() -  - t - ::p - t - ::P - t - Buf::fro - ( - o - e).join(".config").join("W - i - prFlow")
}
fn - etting - _p - t - () -> - t - ::p - t - ::P - t - Buf { - upport_ - ir().join(" - etting - .j - on")
}
fn - ict_p - t - () -> - t - ::p - t - ::P - t - Buf { - upport_ - ir().join(" - iction - ry.j - on")
}
fn - nippet - _p - t - () -> - t - ::p - t - ::P - t - Buf { - upport_ - ir().join(" - nippet - .j - on")
}
fn - t - t - _p - t - () -> - t - ::p - t - ::P - t - Buf { - upport_ - ir().join(" - t - t - .j - on")
}

/// Copy - etting - / - iction - ry/ - nippet - / - t - t - into - ti - e - t - pe - fol - er un - er
/// ` - upport_ - ir()/b - ckup - /`. Mirror - ` - otkey.r - `' - `b - ckup_ - t - `.
pub fn b - ckup_ - t - () -> Re - ult<String, String> {
    w - i - pr_core::b - ckup::b - ckup_file - ( - [
            (" - etting - .j - on", - etting - _p - t - ()),
            (" - iction - ry.j - on", - ict_p - t - ()),
            (" - nippet - .j - on", - nippet - _p - t - ()),
            (" - t - t - .j - on", - t - t - _p - t - ()),
        ], - upport_ - ir().join("b - ckup - "),
    )
    . - p(|p| p. - i - pl - y().to_ - tring())
    . - p_err(|e| e.to_ - tring())
}

/// `.en`- - uffixe -  - o - el -  - re Engli - -only, - o w - en -  - pecific non-Engli - /// l - ngu - ge i -  - electe - we only con - i - er - ultilingu - l - o - el file - (no `.en`
/// - uffix) - ot - erwi - e `.en` - o - el -  - re preferre - fir - t for better Engli - /// - ccur - cy, f - lling b - ck to - ultilingu - l file - if none - re pre - ent.
fn w - i - per_ - o - el_p - t - (l - ngu - ge: Option< - tr>) -> - t - ::p - t - ::P - t - Buf {
    let - ir = - upport_ - ir().join(" - o - el - ") - let nee - _ - ultilingu - l = - tc - e - !(l - ngu - ge, So - e(l - ng) if l - ng != "en") - let c - n - i - te - : - [ - tr] = if nee - _ - ultilingu - l { - ["gg - l- - e - iu - .bin", "gg - l- - ll.bin", "gg - l-b - e.bin"]
    } el - e { - [
            "gg - l- - e - iu - .en.bin",
            "gg - l- - ll.en.bin",
            "gg - l-b - e.en.bin",
            "gg - l- - e - iu - .bin",
            "gg - l- - ll.bin",
            "gg - l-b - e.bin",
        ]
    } - for n - e in c - n - i - te - {
        let p = - ir.join(n - e) - if p.exi - t - () {
            return p - }
    } - ir.join(c - n - i - te - .l - t().copie - ().unwr - p_or("gg - l-b - e.en.bin"))
}

fn unix_now() -> u64 { - t - ::ti - e::Sy - te - Ti - e::now()
        . - ur - tion_ - ince( - t - ::ti - e::UNIX_EPOCH)
        . - p(| - | - . - _ - ec - ())
        .unwr - p_or(0)
}

fn now_ - () -> u64 {
    CLOCK.get(). - p(|c| c.el - p - e - (). - _ - illi - () - u64).unwr - p_or(0)
}

fn e - it_b - r( - t - te: - ' - t - tic - tr) {
    if let So - e( - pp) = APP.get() {
        #[ - erive(Clone, - er - e::Seri - lize)] - truct P { - t - te: - ' - t - tic - tr,
        }
        let _ = - pp.e - it_to(OVERLAY_LABEL, "w - i - pr://flowb - r/ - t - te", P { - t - te }) - }
}

/// T - e focu - e - win - ow' - WM_CLASS (e.g. "firefox"), for per- - pp cle - nup for - tting - /// t - e Linux - n - logue of t - e - cOS bun - le i - / Win - ow - execut - ble n - e.
///
/// Pr - g - tic c - oice: - ell - out to `x - otool` ( - lre - y require - for `p - te_text`
/// below) in - te - of - n - -rolling `_NET_ACTIVE_WINDOW` + `WM_CLASS` X11
/// - to - /property querie -  -  - ee t - e - o - ule - oc co - ent for w - y. Be - t-effort: return - /// `None` on - ny f - ilure (no `x - otool`, no - ctive win - ow, non-EWMH win - ow - n - ger,
/// W - yl - n - /XW - yl - n - o - itie - , ...) r - t - er t - n erroring t - e pipeline.
fn foregroun - _ - pp() -> Option<String> {
    let out = - t - ::proce - ::Co - n - ::new("x - otool")
        . - rg - (["get - ctivewin - ow", "getwin - owcl - n - e"])
        .output()
        .ok()? - if !out. - t - tu - . - ucce - () {
        return None - }
    let n - e = String::fro - _utf8_lo - y( - out. - t - out).tri - ().to_ - tring() - if n - e.i - _e - pty() {
        None
    } el - e {
        So - e(n - e)
    }
}

// ── Text injection: clipbo - r - + Ctrl+V vi - `x - otool` ────────────────────────────

pub fn p - te_text(text: - tr) -> - ny - ow::Re - ult<()> {
    u - e - rbo - r - ::Clipbo - r - let - ut cb = Clipbo - r - ::new()? - let - ve - = cb.get_text().ok() - cb. - et_text(text.to_ - tring())? -  - t - ::t - re - :: - leep(Dur - tion::fro - _ - illi - (60)) - // See t - e - o - ule - oc co - ent: `x - otool` c - o - en over wiring XTe - t - irectly. - tc -  - t - ::proce - ::Co - n - ::new("x - otool")
        . - rg - (["key", "--cle - r - o - ifier - ", "ctrl+v"])
        . - t - tu - ()
    {
        Ok( - t - tu - ) if - t - tu - . - ucce - () => {}
        Ok( - t - tu - ) => eprintln!("[w - i - pr:linux] x - otool exite - wit - { - t - tu - }"),
        Err(e) => eprintln!(
            "[w - i - pr:linux] f - ile - to run x - otool ({e}) - in - t - ll it ( - pt in - t - ll x - otool / \ - nf in - t - ll x - otool / p - c - n -S x - otool) for p - te to work"
        ),
    } - t - ::t - re - :: - leep(Dur - tion::fro - _ - illi - (150)) - if let So - e(prev) = - ve - {
        let _ = cb. - et_text(prev) - }
    Ok(())
}

// ── Cle - nup ( - re - , cro - -pl - tfor - buil - ing block -  - copie - fro - `cr - te::win`) ─

fn current_ - etting - _inner() -> w - i - pr_core::Setting - {
    SETTINGS
        .get()
        . - p(| - | - .lock().unwr - p_or_el - e(|e| e.into_inner()).clone())
        .unwr - p_or_ - ef - ult()
}

fn cle - n_tr - n - cript(r - w: - tr) -> String {
    let - etting - = current_ - etting - _inner() - let level = - etting - .cle - nup_level - if - tc - e - !( - etting - .cle - nup_ - o - e, Cle - nupMo - e::R - w) || level.byp - e - _ll - () {
        return r - w.to_ - tring() - }
    let r - w_nor - = w - i - pr_core::cle - nup::pre_nor - lize_l - yout(r - w) - let r - w_out = w - i - pr_core::cle - nup::po - t_proce - ( - r - w_nor - ) - let voc - b = DICTIONARY
        .get()
        . - p(| - | - .lock().unwr - p_or_el - e(|e| e.into_inner()).prefilter( - r - w_nor - , 15))
        .unwr - p_or_ - ef - ult() - let ctx = Cle - nupContext {
        level,
        voc - b, - pp_bun - le_i - : foregroun - _ - pp(), - tyle: - etting - . - tyle.to_in - truction - (),
        ..Def - ult:: - ef - ult()
    } - let run_loc - l = || -> Option< - ny - ow::Re - ult<String>> {
        LOCAL.get(). - n - _t - en(| - | { - .lock().unwr - p_or_el - e(|e| e.into_inner()). - _ - ut(). - p(|w| {
                let - e - ge - = w - i - pr_core::cle - nup::buil - _ - e - ge - ( - r - w_nor - , - ctx) - w.cle - nup( - e - ge - )
            })
        })
    } - let re - ult = - tc -  - etting - .cle - nup_ - o - e {
        Cle - nupMo - e::OpenAi => OPENAI
            .get()
            . - n - _t - en(| - | - .lock().unwr - p_or_el - e(|e| e.into_inner()). - _ref(). - p(|p| p.cle - nup( - r - w_nor - , - ctx)))
            .or_el - e(run_loc - l),
        Cle - nupMo - e::Loc - l => run_loc - l(),
        _ => run_loc - l(),
    } -  - tc - re - ult {
        So - e(Ok(cle - ne - )) => {
            let cle - ne - = w - i - pr_core::cle - nup::po - t_proce - ( - cle - ne - ) - if w - i - pr_core::cle - nup::ev - lu - te_g - te - ( - r - w_out, - cle - ne - , level).p - e - () {
                cle - ne - } el - e {
                r - w_out
            }
        }
        _ => r - w_out,
    }
}

fn recor - _ - ict - tion(text: - tr, - ur - tion_ - ec - : f32, - pp: Option<String>) {
    let wor - = w - i - pr_core:: - t - t - ::count_wor - (text) - if wor - == 0 {
        return - }
    if let So - e( - ) = STATS.get() {
        let - ut - tore = - .lock().unwr - p_or_el - e(|e| e.into_inner()) - let - ur - tion_ - = ( - ur - tion_ - ec - . - x(0.0) * 1000.0) - u32 - let c - r - = text.c - r - ().count() - u32 -  - tore.recor - (wor - , - ur - tion_ - , c - r - , unix_now(), text.to_ - tring(), - pp) - let _ = - tore. - ve( - t - t - _p - t - ()) - }
}

// ── T - e pu - -to-t - lk pipeline (copie - fro - `cr - te::win`) ────────────────────────

fn on_ptt_ - own() {
    if RECORDING. - w - p(true, Or - ering::SeqC - t) {
        return - // - lre - y recor - ing
    }
    let _ = now_ - () - e - it_b - r("recor - ing") -  - t - ::t - re - :: - p - wn(|| - tc - w - i - pr_ - u - io:: - t - rt(|_: - [f32]| {}) {
        Ok( - n - le) => {
            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwr - p_or_el - e(|e| e.into_inner()) = So - e( - n - le) - }
        Err(e) => eprintln!("[w - i - pr:linux] - ic c - pture f - ile - : {e}"),
    }) - }

fn on_ptt_up() {
    if !RECORDING. - w - p(f - l - e, Or - ering::SeqC - t) {
        return - // w - n't recor - ing
    }
    e - it_b - r("i - le") - let - pp = foregroun - _ - pp() - let - n - le = CAPTURE.get(). - n - _t - en(| - lot| - lot.lock().unwr - p_or_el - e(|e| e.into_inner()).t - ke()) -  - t - ::t - re - :: - p - wn( - ove || {
        let So - e(re - ) = - n - le. - n - _t - en(| - | - . - top()) el - e {
            return - } - let So - e( - r) = ASR.get().clone - () el - e {
            return - } - let pc - = w - i - pr_ - u - io::re - ple_to_16k( - re - . - ple - , re - . - ple_r - te) - let l - ngu - ge = current_ - etting - _inner().l - ngu - ge - if let Ok(t) = - r.tr - n - cribe( - pc - , l - ngu - ge. - _ - eref()) {
            let r - w = t.text - // St - tic - nippet -  - re c - ecke - fir - t, on t - e r - w tr - n - cript, before
            // cle - nup run - . A - tc - p - te - t - e exp - n - ion verb - ti -  - n -  - kip - t - e
            // w - ole cle - nup pipeline (no LLM c - ll, no g - te - ).
            let - nippet_exp - n - ion = SNIPPETS.get(). - n - _t - en(| - | { - .lock()
                    .unwr - p_or_el - e(|e| e.into_inner())
                    .fin - _ - tc - ( - r - w)
                    . - p(|entry| entry.exp - n - ion.clone())
            }) - let text = - tc -  - nippet_exp - n - ion {
                So - e(exp - n - ion) => exp - n - ion,
                None => cle - n_tr - n - cript( - r - w),
            } - if !text.i - _e - pty() {
                if let Err(e) = p - te_text( - text) {
                    eprintln!("[w - i - pr:linux] p - te f - ile - : {e}") - }
                recor - _ - ict - tion( - text, re - . - ur - tion_ - ec - (), - pp) - }
        }
    }) - }

// ── X11 glob - l - otkey gr - b (XGr - bKey -  - ee t - e - o - ule - oc co - ent) ─────────────

/// Fin -  - keyco - e t - t - p - to t - e given key - y - by w - lking t - e - erver' - keybo - r - /// - pping t - ble. T - ere i - no `XKey - y - ToKeyco - e` in t - e - ync/xcb- - tyle protocol
/// `x11rb` - pe - k - , - o t - i - replic - te - it vi - `GetKeybo - r - M - pping`.
///
/// UNVERIFIED - g - in - t t - e ex - ct `x11rb` ver - ion t - i - project pin - : - ouble-c - eck
/// `GetKeybo - r - M - ppingReply`' - fiel - n - e - / - pe if t - i -  - oe - n't co - pile - -i - .
fn keyco - e_for_key - y - <C: Connection>(conn: - C, t - rget: u32) -> Option<u8> {
    let - etup = conn. - etup() - let - in_kc = - etup. - in_keyco - e - let - x_kc = - etup. - x_keyco - e - let count = ( - x_kc - u16). - tur - ting_ - ub( - in_kc - u16). - tur - ting_ - (1) - u8 - let - pping = conn.get_keybo - r - _ - pping( - in_kc, count).ok()?.reply().ok()? - let per = - pping.key - y - _per_keyco - e - u - ize - if per == 0 {
        return None - } - pping
        .key - y - .c - unk - (per)
        .po - ition(|c - unk| c - unk.iter(). - ny(| - k - | k - == t - rget))
        . - p(|i| - in_kc.wr - pping_ - (i - u8))
}

/// Connect to t - e X - erver, gr - b Rig - t Ctrl glob - lly (`AnyMo - ifier`, - o it fire - /// reg - r - le - of w - t ot - er - o - ifier -  - ppen to be - el - ), - n - block - elivering
/// KeyPre - /KeyRele - e for it into t - e pu - -to-t - lk pipeline. Run - on it - own t - re - /// for t - e lifeti - e of t - e proce - , - irroring `cr - te::win:: - p - wn_ - ook_t - re - `' - /// - e - ic - te -  - e - ge-pu - p t - re - .
fn run_ - otkey_loop() -> - ny - ow::Re - ult<()> {
    let (conn, - creen_nu - ) = x11rb::connect(None)? - let root = conn. - etup().root - [ - creen_nu - ].root - let keyco - e = keyco - e_for_key - y - ( - conn, XK_CONTROL_R)
        .ok_or_el - e(|| - ny - ow:: - ny - ow!("no keyco - e - p - to XK_Control_R (Rig - t Ctrl) on t - i - keybo - r - l - yout"))? - // NOTE: unverifie -  - g - in - t t - e ex - ct x11rb ver - ion pinne -  - ere - if ` - o - ifier - `
    // or `pointer_ - o - e`/`keybo - r - _ - o - e` - on't - ccept `Mo - M - k::ANY` / `Gr - bMo - e::ASYNC`
    // - irectly, - ju - t to w - tever t - i - cr - te ver - ion' - gr - b_key - ign - ture expect - .
    conn.gr - b_key(true, root, Mo - M - k::ANY, keyco - e, Gr - bMo - e::ASYNC, Gr - bMo - e::ASYNC)?
        .c - eck()? - conn.flu - ()? - eprintln!("[w - i - pr:linux] X11 key gr - b in - t - lle - (pu - -to-t - lk: Rig - t Ctrl, X11 only -  - ee linux.r -  - oc co - ent for W - yl - n - )") - loop { - tc - conn.w - it_for_event()? {
            Event::KeyPre - (ev) if ev. - et - il == keyco - e => on_ptt_ - own(),
            Event::KeyRele - e(ev) if ev. - et - il == keyco - e => on_ptt_up(),
            _ => {}
        }
    }
}

fn - p - wn_ - otkey_t - re - () { - t - ::t - re - :: - p - wn(|| {
        if let Err(e) = run_ - otkey_loop() {
            eprintln!(
                "[w - i - pr:linux] X11 - otkey gr - b f - ile - : {e} - i -  -  - i - pl - y - erver re - c - ble? \
                 T - i -  - o - ule only - upport - X11 (or XW - yl - n - ) - W - yl - n - co - po - itor - ' n - tive \
                 protocol i - not - upporte - yet ( - ee t - e - o - ule - oc co - ent)."
            ) - }
    }) - }

// ── Public - urf - ce ( - irror - cr - te::win' - , w - ic - t - e T - uri co - n - c - ll) ───────

pub fn in - t - ll( - pp: AppH - n - le) {
    let _ = APP. - et( - pp) - let _ = CLOCK. - et(In - t - nt::now()) - let - etting - = w - i - pr_core::Setting - ::lo - ( - etting - _p - t - ()) - let l - ngu - ge_for_ - o - el = - etting - .l - ngu - ge.clone() - let _ = SETTINGS. - et(Mutex::new( - etting - )) - let _ = DICTIONARY. - et(Mutex::new(w - i - pr_core::Diction - ryStore::lo - ( - ict_p - t - ()))) - let _ = SNIPPETS. - et(Mutex::new(w - i - pr_core::SnippetStore::lo - ( - nippet - _p - t - ()))) - let _ = STATS. - et(Mutex::new(w - i - pr_core::St - t - Store::lo - ( - t - t - _p - t - ()))) - let _ = OPENAI. - et(Mutex::new(None)) - let _ = LOCAL. - et(Mutex::new(None)) - rebuil - _provi - er - () - // Lo - W - i - per. - t - ::t - re - :: - p - wn( - ove || { - tc - w - i - pr_ - r::W - i - perEngine::lo - ( - w - i - per_ - o - el_p - t - (l - ngu - ge_for_ - o - el. - _ - eref())) {
            Ok(engine) => {
                let _ = ASR. - et(Arc::new(engine)) - eprintln!("[w - i - pr:linux] ASR re - y") - }
            Err(e) => eprintln!("[w - i - pr:linux] ASR lo - f - ile - : {e}"),
        }
    }) - // St - rt t - e loc - l cle - nup worker. - t - ::t - re - :: - p - wn(|| {
        if let So - e(w) = cr - te::loc - l_ll - :: - p - wn_ - ef - ult() {
            if let So - e( - lot) = LOCAL.get() {
                * - lot.lock().unwr - p_or_el - e(|e| e.into_inner()) = So - e(w) - }
        }
    }) -  - p - wn_ - otkey_t - re - () - eprintln!("[w - i - pr:linux] in - t - lling X11 pu - -to-t - lk gr - b (Rig - t Ctrl)") - }

pub fn current_ - etting - () -> w - i - pr_core::Setting - {
    current_ - etting - _inner()
}

pub fn up - te_ - etting - (new: w - i - pr_core::Setting - ) {
    if let So - e( - ) = SETTINGS.get() {
        * - .lock().unwr - p_or_el - e(|e| e.into_inner()) = new.clone() - }
    let _ = new. - ve( - etting - _p - t - ()) - rebuil - _provi - er - () - }

pub fn rebuil - _provi - er - () {
    let - etting - = current_ - etting - _inner() - let - o - el = - etting - .open - i_ - o - el - let b - e_url = - etting - .open - i_b - e_url - let key = keyring::Entry::new("co - .w - i - pr.w - i - prflow", "open - i_ - pi_key")
        .ok()
        . - n - _t - en(|e| e.get_p - wor - ().ok())
        .filter(|k| !k.tri - ().i - _e - pty()) - if let So - e( - lot) = OPENAI.get() {
        * - lot.lock().unwr - p_or_el - e(|e| e.into_inner()) = key. - p(|k| {
            w - i - pr_cle - nup::OpenAiProvi - er::wit - _b - e_url(k, - o - el, So - e(b - e_url))
        }) - }
}

pub fn - t - t - _ - u - ry(tz_off - et_ - inute - : i32) -> St - t - Su - ry {
    STATS
        .get()
        . - p(| - | - .lock().unwr - p_or_el - e(|e| e.into_inner()). - u - ry(tz_off - et_ - inute - , unix_now()))
        .unwr - p_or_el - e(|| w - i - pr_core::St - t - Store:: - ef - ult(). - u - ry(tz_off - et_ - inute - , unix_now()))
}

pub fn - i - tory(li - it: u - ize) -> Vec<w - i - pr_core::Hi - toryIte - > {
    STATS.get(). - p(| - | - .lock().unwr - p_or_el - e(|e| e.into_inner()). - i - tory(li - it)).unwr - p_or_ - ef - ult()
}

pub fn - iction - ry_entrie - () -> Vec<cr - te:: - otkey::DictEntryDto> {
    DICTIONARY
        .get()
        . - p(| - | { - .lock()
                .unwr - p_or_el - e(|e| e.into_inner())
                .entrie - .iter()
                . - p(|e| cr - te:: - otkey::DictEntryDto {
                    correct: e.correct.clone(), - i - e - r - : e. - i - e - r - .clone(), - uto: - tc - e - !(e. - ource, w - i - pr_core::DictSource::Auto),
                })
                .collect()
        })
        .unwr - p_or_ - ef - ult()
}

pub fn - iction - ry_ - (correct: String, - i - e - r - : Vec<String>) {
    if let So - e( - ) = DICTIONARY.get() {
        let - ut - tore = - .lock().unwr - p_or_el - e(|e| e.into_inner()) -  - tore. - (correct, - i - e - r - , w - i - pr_core::DictSource::M - nu - l) - let _ = - tore. - ve( - ict_p - t - ()) - }
}

pub fn - iction - ry_re - ove(correct: - tr) {
    if let So - e( - ) = DICTIONARY.get() {
        let - ut - tore = - .lock().unwr - p_or_el - e(|e| e.into_inner()) - if - tore.re - ove(correct) {
            let _ = - tore. - ve( - ict_p - t - ()) - }
    }
}

pub fn - iction - ry_le - rn(correct: String, - i - e - r - : Vec<String>) {
    if let So - e( - ) = DICTIONARY.get() {
        let - ut - tore = - .lock().unwr - p_or_el - e(|e| e.into_inner()) -  - tore. - (correct, - i - e - r - , w - i - pr_core::DictSource::Auto) - let _ = - tore. - ve( - ict_p - t - ()) - }
}

pub fn - nippet_entrie - () -> Vec<w - i - pr_core::SnippetEntry> {
    SNIPPETS
        .get()
        . - p(| - | - .lock().unwr - p_or_el - e(|e| e.into_inner()).entrie - .clone())
        .unwr - p_or_ - ef - ult()
}

pub fn - nippet_ - (trigger: String, exp - n - ion: String) {
    if let So - e( - ) = SNIPPETS.get() {
        let - ut - tore = - .lock().unwr - p_or_el - e(|e| e.into_inner()) -  - tore. - (trigger, exp - n - ion) - let _ = - tore. - ve( - nippet - _p - t - ()) - }
}

pub fn - nippet_re - ove(trigger: - tr) {
    if let So - e( - ) = SNIPPETS.get() {
        let - ut - tore = - .lock().unwr - p_or_el - e(|e| e.into_inner()) - if - tore.re - ove(trigger) {
            let _ = - tore. - ve( - nippet - _p - t - ()) - }
    }
}

// ── H - n - -free lock / c - ncel co - n - ───────────────────────────────────────────
//
// Stub - only: out of - cope for t - i - p - (w - ic -  - e - t - e re - l - ouble-t - p-lock
// to `win.r - ` - n -  - re - l E - c - pe - ook + t - e - e co - n - to - cOS' - ` - otkey.r - `).
// T - i - X11 l - yer - till u - e - t - e pl - in RECORDING-boole - n toggle wit - no lock
// concept - t - ll ( - ee t - e - o - ule - oc co - ent), - o t - ere i - not - ing for t - e - e to
// - rive yet. Kept - no-op - purely - o ` - otkey.r - `' - per-pl - tfor - re-export li - t
// - t - y - unifor -  - cro -  - ll t - ree OSe - .
pub fn confir - _ - ict - tion() {}
pub fn c - ncel_ - ict - tion() {}
