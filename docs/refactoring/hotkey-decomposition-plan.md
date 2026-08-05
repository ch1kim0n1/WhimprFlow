# hotkey.rs Decomposition Plan

## Current State

`src-tauri/src/hotkey.rs` is a **2,735-line / 118 KB god module** that mixes:
- Platform FFI (CoreGraphics event tap on macOS, Windows LL hook, Linux X11)
- Global state management (20+ `OnceLock<Mutex<...>>` singletons)
- Audio capture orchestration
- ASR (Whisper) orchestration
- Cleanup provider orchestration (OpenAI, Anthropic, local llama.cpp)
- Settings persistence
- Dictionary/snippets/workflows/stats storage
- Voice memory recording
- Notes management
- Platform-specific text insertion (paste.rs, win.rs, linux.rs)

## The Problem

1. **Untestable**: Global singletons make unit testing the orchestration layer impossible
2. **Unmaintainable**: 2,735 lines in one file; changes require understanding the entire module
3. **Unclear boundaries**: FFI, state, business logic, and storage are fused
4. **Concurrency hazards**: 20+ global mutable locks with no clear ownership model

## Decomposition Strategy

### Phase 1: Extract Utilities (Low Risk)

Create `src-tauri/src/hotkey_utils.rs`:
- `truncate_chars()` (line 524)
- `unix_now()` (line 392)
- `effective_language()` (line 505)
- `asr_engine_tag()` (line 517)
- File path helpers (`model_path`, `support_dir`, `settings_path`, etc.)

### Phase 2: Extract State Registry (Medium Risk)

Create `src-tauri/src/hotkey_state.rs`:
- Move all `OnceLock<Mutex<...>>` globals into a `struct AppState`
- Implement `AppState::new()` and `AppState::default()`
- Add methods like `state.get_asr()`, `state.set_asr()`, etc.
- This enables dependency injection in tests

```rust
pub struct AppState {
    pub machine: OnceLock<Mutex<StateMachine>>,
    pub clock: OnceLock<Mutex<Instant>>,
    pub fn_is_down: AtomicBool,
    pub tap_port: AtomicPtr<c_void>,
    pub target_app: OnceLock<Mutex<Option<String>>>,
    pub capture: OnceLock<Mutex<Option<whimpr_audio::CaptureHandle>>>,
    pub asr: OnceLock<Mutex<Option<Arc<whimpr_asr::WhisperEngine>>>>,
    pub openai: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>>,
    pub anthropic: OnceLock<Mutex<Option<whimpr_cleanup::AnthropicProvider>>>,
    pub local: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>>>,
    pub settings: OnceLock<Mutex<whimpr_core::Settings>>,
    pub dictionary: OnceLock<Mutex<whimpr_core::DictionaryStore>>,
    pub snippets: OnceLock<Mutex<whimpr_core::SnippetStore>>,
    pub stats: OnceLock<Mutex<whimpr_core::StatsStore>>,
    pub last_texts: OnceLock<Mutex<Option<(String, String)>>>,
    pub workflows: OnceLock<Mutex<whimpr_core::WorkflowStore>>,
    pub voice_memory: OnceLock<Mutex<whimpr_core::VoiceMemory>>,
    // ... etc
}
```

### Phase 3: Extract Orchestration Layer (High Risk)

Create `src-tauri/src/orchestration.rs`:
- `record_dictation()` (line 404)
- `maybe_reload_asr()` (line 799)
- `ensure_asr_loaded()` (line 841)
- `rebuild_providers()` (from lib.rs)
- Cleanup orchestration logic
- Dictionary/snippet/workflow orchestration

### Phase 4: Extract Platform FFI Layer (High Risk)

Create `src-tauri/src/platform/hotkey_macos.rs`:
- Move macOS CoreGraphics FFI
- Event tap callback
- Fn key detection

Create `src-tauri/src/platform/hotkey_windows.rs`:
- Move Windows LL hook FFI
- `SetWindowsHookEx` callback
- Right Ctrl detection

Create `src-tauri/src/platform/hotkey_linux.rs`:
- Move Linux X11 FFI
- XInput2 event handling

### Phase 5: Extract Storage Layer (Low Risk)

Create `src-tauri/src/storage.rs`:
- `settings_path()`, `dict_path()`, etc.
- File I/O for JSON stores
- Backup/restore logic

### Phase 6: Create Integration Tests (New)

Create `src-tauri/tests/integration.rs`:
- Test PTT → capture → transcript → cleanup → paste loop
- Test double-tap lock behavior
- Test cancel-during-cleanup
- Test concurrent dictation triggers
- Test settings persistence across restarts

## Migration Path

1. **Phase 1**: Extract utilities — no behavioral changes, pure code motion
2. **Phase 2**: Introduce `AppState` alongside existing globals — gradual migration
3. **Phase 3**: Create orchestration module that uses `AppState` — wire up gradually
4. **Phase 4**: Extract platform FFI — one platform at a time, test each independently
5. **Phase 5**: Extract storage — low risk, mostly I/O
6. **Phase 6**: Write integration tests to validate the refactored system

## Success Criteria

- No file exceeds 1,000 lines
- No module has more than 3 responsibilities
- Global singletons reduced to < 5 (only truly global state like the app handle)
- Unit tests exist for orchestration layer
- Integration tests exist for PTT loop
- Platform FFI is isolated and testable via mocks

## Estimated Effort

- Phase 1: 2-4 hours
- Phase 2: 8-12 hours
- Phase 3: 16-24 hours
- Phase 4: 24-32 hours (one week for all platforms)
- Phase 5: 4-6 hours
- Phase 6: 16-24 hours

**Total: ~70-102 hours (2-3 weeks)**

## Risks

1. **Concurrency bugs**: Moving from fine-grained locks to a single `AppState` lock could introduce contention
   - Mitigation: Keep fine-grained locks inside `AppState`, don't coarse-grain everything

2. **Platform regressions**: Extracting FFI could break subtle platform behavior
   - Mitigation: Test each platform independently before merging

3. **State initialization order**: `OnceLock` lazy initialization may break if order changes
   - Mitigation: Add explicit initialization sequence in `AppState::new()`

## Alternative: Gradual Extraction

Instead of a big-bang refactor, extract one subsystem at a time:
1. Extract storage (safest, no state)
2. Extract dictionary/snippets/workflows (self-contained)
3. Extract ASR orchestration (well-defined interface)
4. Extract platform FFI last (riskiest)

This reduces risk but extends timeline to 4-6 weeks.