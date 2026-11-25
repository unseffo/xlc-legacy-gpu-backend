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
| 0.2a | Agility SDK + inbox vs Agility + G0 | done | TGT | 2026-11-25 | | tools/legacy_probe/out_tgt_inbox.json, out_tgt_agility.json, tools/fetch_agility.ps1 | Два отдельных прогона; Agility-прогон загрузил DLL из third_party (проверено по пути, exit 0). Итог: inbox SM 6.5 / Agility SM 6.8; cs_6_6 PSO ok только через Agility; cs_6_0 wave PSO ok в обоих. SM 6.5-блокер снят |
| 0.3 | GATE G0: SM 6.6 PSO | done | TGT | 2026-11-25 | | tools/legacy_probe/out_tgt_inbox.json, out_tgt_agility.json | Выполнено в рамках 0.2a (по инструкции владельца): DXC 1.9.2602.17 + dxil.dll 1.9.2602.17, cs_6_6 и cs_6_0+WaveActiveSum, оба HRESULT записаны, вердикт G0 в «Решениях» и «Контрольных воротах» |
| 0.4 | Baseline на DEV | todo | DEV | | | | |
| 0.5 | Ветка + плейбук в репо | done | ANY | 2026-11-25 | 101e8c5 | AGENT_PLAYBOOK.md, PROGRESS.md | |
| 0.6 | TGT: сборка и ранний прогон compute-тестов | todo | TGT | | | | |
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

## Решения (append-only)

- D1..D8 (дефолты) — см. AGENT_PLAYBOOK.md Ч.4; после шагов 1.5/1.6 дублируются сюда в окончательном виде (с выбором D3-A/D3-B).
- **G0 (шаги 0.2a/0.3, 2026-11-25, TGT): PASS через Agility SDK.** Факты (out_tgt_inbox.json / out_tgt_agility.json): inbox-рентайм (System32 D3D12Core.dll 6.2.19041.5794, D3D12SDKVersion=2) сообщает SM 6.5 и отвергает heap-directly-indexed RS (`D3D12SerializeVersionedRootSignature` → 0x80070057 E_INVALIDARG) и cs_6_6 PSO (E_INVALIDARG); Agility SDK 1.620.2 (third_party, D3D12SDKVersion=620) сообщает SM **6.8**, heap-RS ok, **cs_6_6 PSO ok**, cs_6_0 WaveActiveSum PSO ok. Вывод: «SM 6.5» из прогона 0.2 — лимит inbox-рентайма Windows 10 (agility.rs:1-17), не GPU; GTX 1080 + драйвер 32.0.15.8278 поддерживает до SM 6.8. XCL уже грузит Agility SDK (`gpu/agility.rs`), требование SM 6.6 на TGT выполнимо без правок кода. MeshShaderTier=0 — аппаратный факт Pascal (закрывается легаси-бэкендом), wave 32/32 подтверждён, wave-интринсики работают (обе вариации PSO ok).

## Блокеры

- (пусто) — блокер «MaxShaderModel=6.5» снят шагом 0.2a: 6.5 — лимит inbox-рентайма Windows 10; через Agility SDK 1.620.2 GPU поддерживает SM 6.8 (см. «Решения», G0).

## Next

- DEV: шаги **0.4** (baseline), затем **0.7** (WARP wave32). TGT: **0.6** (сборка XCL + ранний прогон compute-тестов; зависимости 0.5 и 0.3/G0 выполнены).
- Если TGT недоступна — работа на DEV идёт до этапа 7; TGT-шаги остаются `todo` («ждёт TGT»).
