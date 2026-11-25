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
- GPU / VRAM: — (из 0.1)
- Драйвер: — (из 0.1)
- ОС: —
- Rust: —
- Windows SDK / DXC: —
- MeshShaderTier / WaveLaneCountMax / макс. SM: — (из 0.2: `tools/legacy_probe/out_tgt.json`)
- G0 (SM 6.6 PSO): — (из 0.3)
- Каталог артефактов (вне репо): —

### Общее
- Ветка: `legacy-gpu-backend`
- Remote для синхронизации DEV↔TGT: — (заполнить; без remote передача журнала — вручную)
- Команда smoke: `.\scripts\smoke.ps1 -Game <folder>` (заполнить фактический путь к тайтлу)
- Запуск тайтла: — (заполнить)

---

## Контрольные ворота

| Ворота | Шаг | Статус (pass/fail/условный/—) | Дата | Ключевые цифры / артефакты | Подтверждение владельца |
|---|---|---|---|---|---|
| G0 | 0.3 | — | | | не требуется |
| G1 | 3.7 | — | | | не требуется |
| G2 | 4.6 | — | | допуск: ___ (записать ДО прогона) | требуется |
| G3 | 5.5 | — | | | не требуется |
| G4 | 7.2/7.3 | — | | | требуется |

---

## Статус шагов

| Шаг | Название | Статус | Машина | Дата | Коммит | Артефакты | Примечание |
|---|---|---|---|---|---|---|---|
| 0.1 | Проба: устройство TGT | todo | TGT | | | | |
| 0.2 | Проба: фичи D3D12 | todo | TGT | | | | |
| 0.3 | GATE G0: SM 6.6 PSO | todo | TGT | | | | |
| 0.4 | Baseline на DEV | todo | DEV | | | | |
| 0.5 | Ветка + плейбук в репо | todo | ANY | | | | |
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

## Решения (append-only)

- D1..D8 (дефолты) — см. AGENT_PLAYBOOK.md Ч.4; после шагов 1.5/1.6 дублируются сюда в окончательном виде (с выбором D3-A/D3-B).

## Блокеры

- (пусто)

## Next

- DEV: шаги **0.4**, затем **0.7**, **0.5**. TGT: шаги **0.1** → **0.2** → **0.3** (G0), затем **0.6**.
- Если TGT недоступна — работа на DEV идёт до этапа 7; TGT-шаги остаются `todo` («ждёт TGT»).
