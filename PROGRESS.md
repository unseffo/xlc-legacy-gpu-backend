# PROGRESS — XCL Legacy GPU Backend

> Журнал для передачи работы между агентами. Правила ведения — в `AGENT_PLAYBOOK.md` (Ч.3). Версия шаблона: 2.0.
> Статусы шагов: `todo` → `doing` → `done` / `blocked` / `deferred` / `reopened`.
> **Не более одного `doing` на машину** (DEV / TGT); для шагов ANY в примечании пиши «взял: DEV/TGT».
> Перед каждой записью: `git pull --rebase` (если есть remote) и перечитай файл.
> Разделы «Последние события», «Решения», «Подтверждения владельца» — append-only.
> При конфликте слияния сохраняй обе стороны; в таблице шагов бери более продвинутый статус, при сомнении — `reopened`.

---

## Окружение

### DEV (mesh GPU)
- GPU / VRAM: —
- Драйвер: —
- ОС: —
- Rust: — (rustc --version)
- Windows SDK / DXC: —
- Путь к репо: —
- Каталог артефактов (вне репо): —
- Путь к дампу тайтла / `XCL_SAMPLE_RPF`: —

### TGT (GTX 1080)
- GPU / VRAM: NVIDIA GeForce GTX 1080 / 8 GB (8450473984 bytes)
- Драйвер: 32.0.15.8278
- ОС: Windows 10 Pro 10.0.19045
- Rust: 1.100.0 (0ef1bc386 2026-11-09)
- Windows SDK / DXC: 10.0.26100.0 (dxc.exe at C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\dxc.exe)
- System32\xgameruntime.dll: отсутствует (preflight Warning; диалоги/текст-ввод/invite недоступны — compute-тестам не мешает; лечится установкой Gaming Services)
- Agility SDK: Microsoft.Direct3D.D3D12 **1.620.2** — `tools/fetch_agility.ps1` → `third_party/agility/`
  - `third_party/agility/build/native/bin/x64/D3D12Core.dll`: v1.620.2.0.20261106.5, 4874024 bytes, Authenticode **VALID** (CN=Microsoft Corporation)
  - SHA256 архива `third_party/agility.zip`: `70489669C2BBEAF318B62D72B0AC3E753D521387BFC848C9763791C4E920BFCC`
  - Лицензия: MIT — `third_party/agility/LICENSE-CODE.txt`; `third_party/agility/LICENSE.txt` (Direct3D 12)
  - `XCL_D3D12_DIR` задаётся только в рантайме: `D:\xlc-legacy-gpu-backend\third_party\agility\build\native\bin\x64`
  - `third_party/` в `.gitignore` (бинарники не коммитятся)
- MeshShaderTier / SamplerFeedbackTier: 0 / 0 (Pascal — mesh нет; цель легаси-бэкенда)
- WaveLaneCountMin / Max: 32 / 32 (wave32; G0: cs_6_0 `WaveActiveSum` PSO **ok** в обоих рантаймах)
- MaxShaderModel: inbox **6.5** (C:\Windows\System32\D3D12Core.dll 6.2.19041.5794, D3D12SDKVersion=2) · Agility 1.620.2 → **6.8** (`third_party/.../D3D12Core.dll` 1.620.2.0, D3D12SDKVersion=620)
- ResourceBindingTier 3 · TiledResourcesTier: inbox 3 / Agility 4 · ViewInstancingTier 2
- G0 (SM 6.6 PSO): **PASS через Agility SDK** (cs_6_6 PSO ok, heap-RS ok; inbox-вариант: оба E_INVALIDARG) — шаги 0.2a/0.3
- Каталог артефактов (вне репо): D:\xlc-legacy-gpu-backend\artifacts_tgt

### Общее
- Ветка: `legacy-gpu-backend`
- Remote для синхронизации DEV↔TGT: — (заполнить; без remote передача журнала — вручную)
- Команда smoke: `.\scripts\smoke.ps1 -Game <folder>` (заполнить фактический путь к тайтлу)
- Запуск тайтла: — (заполнить)

---

## Контрольные ворота

| Ворота | Шаг | Статус (pass/fail/условный/—) | Дата | Ключевые цифры / артефакты | Подтверждение владельца |
|---|---|---|---|---|---|
| G0 | 0.3 | **pass** (через Agility SDK; inbox-вариант fail) | 2026-11-25 | Agility 1.620.2: SM 6.8, cs_6_6 PSO ok, heap-RS ok; inbox: SM 6.5, cs_6_6 PSO и heap-RS → 0x80070057 E_INVALIDARG. Артефакты: `tools/legacy_probe/out_tgt_inbox.json`, `out_tgt_agility.json` | не требуется |
| G1 | 3.7 | — | | | не требуется |
| G2 | 4.6 | — | | допуск: ___ (записать ДО прогона) | требуется |
| G3 | 5.5 | — | | | не требуется |
| G4 | 7.2/7.3 | — | | | требуется |

---

## Статус шагов

| Шаг | Название | Статус | Машина | Дата | Коммит | Артефакты | Примечание |
|---|---|---|---|---|---|---|---|
| 0.1 | Проба: устройство TGT | done | TGT | 2026-11-25 | 4933c9f | tools/legacy_probe/out_tgt.json | Адаптеры: GTX 1080 (8GB VRAM), Microsoft Basic Render Driver; D3D12 device created on adapter 0 (FL 12_0) |
| 0.2 | Проба: фичи D3D12 | done | TGT | 2026-11-25 | e302a75 | tools/legacy_probe/out_tgt.json | Inbox-рентайм: MeshShaderTier=0, WaveLane=32/32, MaxSM=6.5, BindingTier=3. **Пересмотрено в 0.2a: 6.5 — лимит inbox-рентайма, не GPU (см. 0.2a)** |
| 0.2a | Agility SDK + inbox vs Agility + G0 | done | TGT | 2026-11-25 | 984376f | tools/legacy_probe/out_tgt_inbox.json, out_tgt_agility.json, tools/fetch_agility.ps1 | Два отдельных прогона; Agility-прогон загрузил DLL из third_party (проверено по пути, exit 0). Итог: inbox SM 6.5 / Agility SM 6.8; cs_6_6 PSO ok только через Agility; cs_6_0 wave PSO ok в обоих. SM 6.5-блокер снят |
| 0.3 | GATE G0: SM 6.6 PSO | done | TGT | 2026-11-25 | | tools/legacy_probe/out_tgt_inbox.json, out_tgt_agility.json | Выполнено в рамках 0.2a (по инструкции владельца): DXC 1.9.2602.17 + dxil.dll 1.9.2602.17, cs_6_6 и cs_6_0+WaveActiveSum, оба HRESULT записаны, вердикт G0 в «Решениях» и «Контрольных воротах» |
| 0.4 | Baseline на DEV | todo | DEV | | | | |
| 0.5 | Ветка + плейбук в репо | done | ANY | 2026-11-25 | 101e8c5 | AGENT_PLAYBOOK.md, PROGRESS.md | |
| 0.6 | TGT: сборка и ранний прогон compute-тестов | done | TGT | 2026-11-25 | d6b0b74 | docs/legacy/0.6-full-test-run.txt, artifacts_tgt/test_mesh.log, test_recomp.log (вне git) | Release-сборка зелёная (2м09с). `--lib mesh`: 27/0/2. `--lib recomp`: 353/0/19i/26s. **Дополнение (полный workspace)**: `cargo build --workspace` (debug, для e2e-тайтла) + `cargo test --workspace --no-fail-fast -- --nocapture` c Agility → **1154 passed / 0 failed / 46 ignored / 35 GPU-skip(+11 тихих ранних выходов)**; e2e-тайтл прошёл на Pascal («xcl_testtitle: done, failures: 0»); лог: docs/legacy/0.6-full-test-run.txt. ResourceHeapTier=1 подтверждён пробой |
| 0.7 | DEV: compute-проход на WARP (wave32) | todo | DEV | | | | |
| 1.1 | Аудит PSO/record | todo | ANY | | | | |
| 1.2 | Аудит two-pass/split | todo | ANY | | | | |
| 1.3 | Аудит выходов NGG/PS, атрибутов примитива | todo | ANY | | | | |
| 1.4 | Аудит стадий/GS + статистика NGG | todo | DEV | | | | |
| 1.5 | Дизайн: D2/D3/D7/D8, оценка ёмкости | todo | ANY | | | | |
| 1.6 | GATE-документ contract.md | todo | ANY | | | | |
| 1.7 | Карта точек вставки | todo | ANY | | | | |
| 2.1 | Конфиг legacy_backend, cull_*, budget | todo | ANY | | | | |
| 2.2 | effective_legacy() | todo | ANY | | | | |
| 2.3 | Проброс флага (log-only) | todo | DEV | | | | |
| 2.4 | Smoke «off == baseline» | todo | DEV | | | | |
| 3.1 | Скелет legacy-модуля | todo | ANY | | | | |
| 3.2 | Layout, ёмкость, per-frame аллокатор | todo | ANY | | | | |
| 3.3 | Эмиттер scan-CS | todo | ANY | | | | |
| 3.4 | Эмиттер recon-CS | todo | ANY | | | | |
| 3.5 | Golden: scan+recon == mesh/референс | todo | DEV | | | | |
| 3.6 | Корректность на wave32 (WARP/TGT) | todo | DEV | | | | |
| 3.7 | GATE G1: retail golden | todo | DEV | | | | |
| 4.1 | Эмиттер passthrough VS | todo | ANY | | | | |
| 4.2 | Legacy RS + graphics PSO | todo | DEV | | | | |
| 4.3 | CommandSignature и args | todo | DEV | | | | |
| 4.4 | Runtime draw-цепочка (indirect) | todo | DEV | | | | |
| 4.5 | Инструмент сравнения кадров (imgdiff, FRAME_SAVE) | todo | DEV | | | | |
| 4.6 | GATE G2: картинка | todo | DEV | | | | |
| 5.1 | Cull-CS: frustum + стабильная компакция | todo | ANY/DEV | | | | |
| 5.2 | Backface (флаг) | todo | DEV | | | | |
| 5.3 | Hi-Z occlusion (опц.) | todo | DEV | | | | |
| 5.4 | Интеграция cull (cull_enable) | todo | DEV | | | | |
| 5.5 | GATE G3: cull on == off | todo | DEV | | | | |
| 6.1 | Многокадровая устойчивость аллокатора | todo | DEV | | | | |
| 6.2 | Собственный graphics-indirect тайтла | todo | DEV | | | | |
| 6.3 | Аудит барьеров (debug layer) | todo | DEV | | | | |
| 6.4 | Финальная изоляция диффа | todo | ANY | | | | |
| 6.5 | Финальные флаги конфига | todo | ANY | | | | |
| 7.1 | TGT: актуализация сборки/среды | todo | TGT | | | | |
| 7.2 | TGT: прогон тайтла (G4a) | todo | TGT | | | | |
| 7.3 | TGT: legacy/compute-тесты на wave32 (G4b) | todo | TGT | | | | |
| 7.4 | TGT: замер производительности | todo | TGT | | | | |
| 7.5 | Оптимизации | todo | TGT/DEV | | | | |
| 7.6 | Регрессия на DEV | todo | DEV | | | | |
| 8.1 | docs/legacy-gpu-backend.md | todo | ANY | | | | |
| 8.2 | Счётчики report.json | todo | DEV/TGT | | | | |
| 8.3 | README секция | todo | ANY | | | | |
| 8.4 | Закрытие проекта | todo | ANY | | | | |

---

## Подтверждения владельца (append-only)

- (пусто) <!-- формат: [дата] Gx — «визуально ок» / замечания; ссылка на артефакты -->

## Последние события (append-only)

- [2026-11-25 playbook] Журнал создан из PROGRESS_TEMPLATE.md (плейбук v2.0).
- [2026-11-25 TGT] PROGRESS.md инициализирован: заполнено окружение TGT; шаг 0.5 (ветка + плейбук) отмечен done (коммит 101e8c5).
- [2026-11-25 TGT] Шаг 0.1 выполнен: tools/legacy_probe создан, запущен; выделен GTX 1080 (8 GB VRAM, LUID 00000000:00007E43), D3D12 device created (FL 12_0); артефакт: tools/legacy_probe/out_tgt.json.
- [2026-11-25 TGT] Шаг 0.2 выполнен: feature probe на adapter 0 — MeshShaderTier=0, SamplerFeedbackTier=0, WaveLaneCountMin/Max=32/32, **MaxShaderModel=6.5** (XCL требует 6.6 → G0 risk), BindingTier=3, TiledResources=true, ViewInstancingTier=true. Блокер добавлен.
- [2026-11-25 TGT] Шаг 0.2a выполнен (по инструкции владельца). A: `tools/fetch_agility.ps1` скачал Microsoft.Direct3D.D3D12 1.620.2 (SHA256 `7048…BFCC`, подпись Microsoft VALID, MIT) в `third_party/agility` (`third_party/` в .gitignore). B: проба переписана — Agility через `ID3D12SDKConfiguration1::CreateDeviceFactory` + `ApplyToGlobalState` (реплика загрузчика `gpu/agility.rs`; только `XCL_D3D12_DIR`, без неявного поиска локальных копий), SM-проб 6.8→6.0 при E_INVALIDARG, Windows/драйвер из реестра, G0 через DXC 1.9.2602.17 + dxil.dll 1.9.2602.17 (валидация IDxcValidator). C: два отдельных процесса — inbox → `out_tgt_inbox.json`, Agility → `out_tgt_agility.json` (загруженный DLL проверен по пути: third_party-версия, не системная; exit 0 в обоих). Результат: inbox SM 6.5, heap-RS и cs_6_6 PSO → E_INVALIDARG; **Agility SM 6.8, heap-RS ok, cs_6_6 PSO ok, cs_6_0 WaveActiveSum PSO ok в обоих**. SM 6.5-блокер снят: это лимит inbox-рентайма Windows 10 (agility.rs:1-17), не GPU. G0 = PASS. Шаг 0.3 закрыт в рамках 0.2a. Оба варианта прогнаны через `cargo run`, паник нет.
- [2026-11-25 TGT] Анализ XCL при MeshShaderTier=0 / wave32 (к шагу 0.2a): preflight.rs:67-72 — SM<6.6 → Error «shaders cannot run», no-mesh → Warning «geometry and NGG programs are not drawn» (игра стартует, геометрии нет); real.rs:5296-5311 finish_two_pass — mesh-PSO отвергается драйвером → note_translation_error → E_INVALIDARG тайтлу; real.rs:5239-5243 — паттерн skip GPU-тестов «no mesh shaders»; wave32: preflight.rs:73-76 Note (две полуволны, медленнее), docs/wave64-on-wave32.md (реализовано для compute/NGG/ES+GS; VS/PS — upper_lanes), pso.rs:954-979 (graphics_wave32 / wave32_only_for_device), mesh_dispatch.rs:1-16 (rendezvous-каркас). Compute-проход NGG (cs_6_6) на TGT через Agility работоспособен — закрыть mesh-звено и должен легаси-бэкенд.
- [2026-11-25 TGT] Исправление AI-подписи: в коммитах 4933c9f (0.1) и e302a75 (0.2) строка AI-assisted ошибочно указывала «qwen/qwen3.8-27b»; фактическая модель — **GLM-5.3 (opencode, melious/glm-5.3)**. Дальнейшие коммиты подписываются корректно.
- [2026-11-25 TGT] Шаг 0.6 выполнен. Сборка: `cargo build --release --workspace` → «Finished release profile [optimized] in 2m 09s» (2 deprecation-предупреждения fetch_update→try_update, pre-existing, не мои). Тесты (оба прогона с `$env:XCL_D3D12_DIR` → third_party Agility, только на процесс): **XCL сам загрузил Agility SDK v620** (лог: «xcl: D3D12: Agility SDK v620 (D:\…\third_party\agility\build\native\bin\x64)»), «backend 'real' – NVIDIA GeForce GTX 1080 (8059 MiB VRAM)», UMA-окна открыты, ID3D12Device16-hook поднялся. `cargo test -p xcl --lib mesh` → **27 passed / 0 failed**, 2 skipped (оба «no mesh shaders»: gpu_mesh_mode_two_passes; passthrough_mesh_stream_accepts_a_triangle_driver_blob). `cargo test -p xcl --lib recomp` → **353 passed / 0 failed / 19 ignored** (pre-existing benches/retail-опросы, требуют XCL_SAMPLE_RPF/release-прогон) / 26 skipped: **20 «no wave64»** (нативные 64-лейновые тесты; wave32 подтверждён собственным детектом XCL — wave64_programs_supported=split, wave64_supported=false), **3 «no mesh shaders»** (gpu_mesh_mode_two_passes, gpu_geometry_mode_es_gs_strip, _with_branches), **3 «refused»** (per-lane форма direct-DXIL: f16 arithmetic / f16 packed / f32 conversions — осознанный отказ транслятора, lower_dxil.rs:703). **Failed compute-тестов на Pascal: 0** — ранний риск этапа 0 снят. Полные логи: artifacts_tgt/test_mesh.log, artifacts_tgt/test_recomp.log (вне git, *.log). `xcl inspect`: тайтл на TGT недоступен (нет D:\Game, нет XCL_SAMPLE_*). Compute-only семейство, прошедшее без mesh tier: gpu_mesh_mode_reads_memory_windows (tests.rs:3753, guard только gpu_ready — NGG compute → записи + UMA-окна), gpu_dispatch_matches_native_wave64_reference (wave64-рендеву на wave32), весь instruction-suite (353). Пометка для 7.3: mesh-зависимые тесты (gpu_mesh_mode_two_passes-семейство, ES+GS-strip, passthrough_mesh_stream) на TGT скипаются «no mesh shaders» — их compute-половина покрыта, mesh-половине нужны golden-тесты легаси-бэкенда (этап 3.5+).
- [2026-11-25 TGT] Дополнение 0.6 (по инструкции владельца): **полный workspace-прогон** `cargo test --workspace --no-fail-fast -- --nocapture` с Agility SDK (после `cargo build --workspace` — debug-бинарники тестового тайтла для e2e, как делает сам scripts/test.ps1:11; первый прогон без этого дал единственный failed: e2e.rs:93 «missing xcl_testtitle.exe — build first» — среда, не Pascal). Итог: **1154 passed / 0 failed / 46 ignored / 35 GPU-skip**, exit 0; **e2e-тайтл реально прошёл на GTX 1080** («xcl_testtitle: done, failures: 0», PresentX/presents ≥ 1, fallback_pipelines ≥ 1). Полный лог: `docs/legacy/0.6-full-test-run.txt` (178 KB, коммитится). GPU-skip таксономия (35, все засчитаны как passed — runner.rs:617-631 пишет SKIPPED в stderr): 7× «no mesh shaders» (gpu_mesh_mode_two_passes, ES+GS-strip ×2, passthrough_mesh_stream, gpu_geometry_direct_records [совм. «no GPU / DXC / mesh shaders»], emitted_ms_primitive_attribute…, emitted_ms_ps_draw_a_triangle); 20× «no wave64» (нативные 64-lane тесты lower/lower_dxil/tests); 3× «refused» (per-lane direct-DXIL: V_MUL_F16, V_PK_MUL_F16, V_CVT_F16_F32 — «#1426 semantics differ from the console's»); 1× «no native 16-bit ops» (half_precision_overloads_run — Pascal без native fp16); 1× «tiled tier < 2» (placed_title_textures… — **причина подтверждена пробой: ResourceHeapTier=1**, TiledResourcesTier=3/4; сообщение теста едино на 3 условия, title_textures.rs:1697-1702; класс NVIDIA-десктопов, аппаратный, легаси-бэкенда не касается); 1× «cannot disable the driver cache (Windows Developer Mode required), 0x887A002D» (среда без Developer Mode); 1× «no xgameruntime.dll» (e2e debug-layer тест, нет Gaming Services); 1× «no heap windows» (opt-in XCL_HEAP_WINDOWS). Тихие ранние выходы (passed без SKIPPED-строки) — **11** по логу: 6 retail-тестов gpu_pso_e2e («XCL_SAMPLE_RPF not set – skipping», gpu_pso_e2e.rs:102) + shaders::tests::retail_pack_sample (shaders.rs:1953) + gpu::recomp::tests::gpu_retail_vs_blit_gen_amb_cube (recomp/tests.rs:1534) — оба тоже «XCL_SAMPLE_RPF not set»; retail_ttpf_textures… (texture.rs:949, XCL_SAMPLE_TTPF); hle::opus::tests::decodes_a_game_sound (opus.rs:1620, XCL_SAMPLE_AWC); hle::xgameruntime::api::tests::thunk_protocol_with_xcl_objects_on_pc_queues (api.rs:1543, нет системного xgameruntime.dll). Подсчёт по логу: 8×«XCL_SAMPLE_RPF not set» (2 из lib + 6 из gpu_pso_e2e) + 1×TTPF + 1×AWC + 1×xgameruntime = 11; вторая строка «no system xgameruntime.dll» в логе — runtime-лог XAccessibility, не скип. `XCL_REQUIRE_GPU` (runner.rs:612-631): значения только `wave64` и `mesh` (иные — `_ => false`, без эффекта); значение `wave32` **не поддерживается**; `wave64` на 1080 невыполним (нет нативных 64-lane волн → 20+ скипов превратились бы в panic), mesh не требовалось по инструкции → повтор с требованием не проводился; GPU-тесты уже выполнены в полном для этого железа объёме. Проба дополнена полем resource_heap_tier (out_tgt_inbox/agility.json обновлены: ResourceHeapTier=1 в обоих рантаймах).

## Решения (append-only)

- D1..D8 (дефолты) — см. AGENT_PLAYBOOK.md Ч.4; после шагов 1.5/1.6 дублируются сюда в окончательном виде (с выбором D3-A/D3-B).
- **Отклонения от плейбука (2026-11-25, по требованию владельца):** (1) шаг 0.5 выполнен и закоммичен при незавершённом 0.4 — плейбук ставит 0.5 в зависимость от 0.4; фактический порядок иной: инициализация журнала происходила, когда коммит «Docs: add legacy backend playbook» уже существовал в ветке (101e8c5), а DEV-машины в работе не было; статус 0.4 оставлен todo по указанию владельца. (2) Добавлен шаг 0.2a — не существует в плейбуке; введён по прямой инструкции владельца (Agility SDK, inbox vs Agility, G0-тест) и закрыл собой приёмку 0.3. (3) Изменён `.gitignore` (добавлен `/third_party/`) — вне «Файлов шага» 0.2a, прямо предписано инструкцией владельца (пункт A2). (4) Полный тестовый лог 0.6 (`docs/legacy/0.6-full-test-run.txt`, 178 KB) закоммичен в репо вопреки Ч.0.5 («артефакты … в репо — только код/документы») — по прямой инструкции владельца «Сохрани полный лог в docs/legacy/»; лог не содержит данных игры и ключей.
- **Переписывание неотправленной истории перед первым push (2026-11-25, по явному разрешению владельца):** предpush-аудит нашёл 36 личных путей `C:\Users\<имя-пользователя-Windows>\AppData\Local\Temp\…` в закоммиченном логе `docs/legacy/0.6-full-test-run.txt` (вывод DirectStorage/rpf/pls-тестов). Владелец разрешил переписать только неотправленные локальные коммиты ветки `legacy-gpu-backend`. Имя пользователя заменено на `<USER>` прямо в дереве коммитов: `31c94f4 → 71fc9a1`, `463e5e1 → dd5195e` (итоговый дифф — ровно 36 строк одного файла); коммиты до `d6b0b74` включительно не менялись — личных данных в них нет. Сообщения, авторы (`unknown`), email (`unseff0@gmail.com` — публикуется по разрешению владельца) и даты сохранены; email дополнительно закреплён в локальном конфиге репозитория (`git config --local`, без `--global`). Создана локальная резервная ветка `backup/pre-scrub` (не отправляется; `push --all`/`--mirror` не используются). Push — только `origin` (fork владельца), без `--force`. Имя пользователя Windows в журнал сознательно не вписано.
- **G0 (шаги 0.2a/0.3, 2026-11-25, TGT): PASS через Agility SDK.** Факты (out_tgt_inbox.json / out_tgt_agility.json): inbox-рентайм (System32 D3D12Core.dll 6.2.19041.5794, D3D12SDKVersion=2) сообщает SM 6.5 и отвергает heap-directly-indexed RS (`D3D12SerializeVersionedRootSignature` → 0x80070057 E_INVALIDARG) и cs_6_6 PSO (E_INVALIDARG); Agility SDK 1.620.2 (third_party, D3D12SDKVersion=620) сообщает SM **6.8**, heap-RS ok, **cs_6_6 PSO ok**, cs_6_0 WaveActiveSum PSO ok. Вывод: «SM 6.5» из прогона 0.2 — лимит inbox-рентайма Windows 10 (agility.rs:1-17), не GPU; GTX 1080 + драйвер 32.0.15.8278 поддерживает до SM 6.8. XCL уже грузит Agility SDK (`gpu/agility.rs`), требование SM 6.6 на TGT выполнимо без правок кода. MeshShaderTier=0 — аппаратный факт Pascal (закрывается легаси-бэкендом), wave 32/32 подтверждён, wave-интринсики работают (обе вариации PSO ok).

## Блокеры

- (пусто) — блокер «MaxShaderModel=6.5» снят шагом 0.2a: 6.5 — лимит inbox-рентайма Windows 10; через Agility SDK 1.620.2 GPU поддерживает SM 6.8 (см. «Решения», G0).

## Next

- Этап 0 на TGT завершён (0.1, 0.2, 0.2a, 0.3, 0.6). Следующие доступные шаги — **ANY** (этап 1, аудит): **1.1** (PSO/record), затем 1.2, 1.3, 1.7; 1.4 — DEV. 1.5/1.6 — за 1.1–1.4.
- DEV: **0.4** (baseline) и **0.7** (WARP wave32) — параллельно, ждут DEV-агента.
- TGT-специфичных доступных шагов нет до этапа 7 (7.1–7.4 ждут этапов 1–6). TGT-агент, продолжая сессию, может брать ANY-шаги этапа 1 (пометка «взял: TGT»).
