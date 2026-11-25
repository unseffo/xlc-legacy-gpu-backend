# AGENT_PLAYBOOK — XCL Legacy GPU Backend

**Проект:** форк `xlc-legacy-gpu-backend` совместимого слоя XCL (Xbox Series X|S → Windows).
**Цель работы:** реализовать «легаси»-графический бэкенд — вместо mesh shaders (compute pass + mesh pass) использовать compute-реконструкцию геометрии + (опционально) GPU culling + `ExecuteIndirect`, чтобы игры шли на GPU **без** mesh shaders (GTX 1080 / Pascal).
**Для кого:** для ИИ-агента (Claude Code / Codex CLI / другой CLI-агент), работающего на машине пользователя.
**Версия плейбука:** 2.0 · 2026-11-25. Номера строк в коде привязаны к коммиту `ecf1376` (ветка `main`); при расхождении ищи по именам символов, а не по строкам.

> **Что изменилось в 2.0 относительно 1.0** (для оператора; агенту читать не обязательно):
> детерминированный порядок примитивов (scan вместо атомарного append, D2); вариант «без VB» (вершины остаются в `res`, D3); аллокатор и оценка ёмкости перенесены в этап 3 (D8); indirect-draw с самого первого runtime-draw (нет «временного draw с полным capacity»); cull — opt-in, без внешних параметров frustum (D5); атрибуты примитива/provoking vertex (D7); G2 усилен (побитовое/PSNR-сравнение RT + визуальное подтверждение владельца); ранний прогон на TGT/WARP (0.6–0.7); спот-чек и `doing` учитывают несколько машин; проверка изоляции диффа на каждом шаге; приёмка аудитов — по таблице проверяемых утверждений. Нумерация шагов изменена (журнал пуст — переносить нечего).

---

## Ч.0. ПРОЧТИ ПЕРВЫМ — протокол агента

У тебя **нет контекста прошлых сессий**. Источники правды, в порядке приоритета:

1. Этот файл (`AGENT_PLAYBOOK.md`) — что и в каком порядке делать.
2. `PROGRESS.md` в корне репозитория — статус каждого шага, окружение, решения, блокеры.
3. `git log` — что реально сделано (каждый шаг = коммит).
4. `AGENTS.md` в корне репозитория — правила разработки самого XCL (обязательны).
5. Код и `HANDOFF.md` — только чтение (редактировать `HANDOFF.md` и `docs/specs/` запрещено).

### Ч.0.1. Bootstrap-цикл (выполни при каждом старте, до любой правки кода)

0. **Узнай свою машину.** Владелец указывает её в стартовом промпте (`DEV` или `TGT`). Если не указана — определи по GPU (`legacy_probe`/диспетчер устройств/`xcl inspect`) и спроси владельца при сомнении. Код машины используй дальше везде.
1. Прочитай этот плейбук целиком.
2. Выполни `git branch --show-current` и `git log --oneline -5`. Если ты не в ветке `legacy-gpu-backend` — остановись и сообщи (работа на `main` запрещена; исключение — шаг 0.5 создаёт ветку).
3. Если настроен remote: `git pull --rebase` (журнал и код могут быть обновлены агентом на другой машине). Конфликт в `PROGRESS.md` разрешай **сохраняя обе стороны** (строки событий — append-only; в таблице шагов бери более «продвинутый» статус, а при спорности — `reopened` с пометкой).
4. Если `PROGRESS.md` отсутствует — создай из `PROGRESS_TEMPLATE.md` (заполни «Окружение» по фактической машине), закоммить по Ч.0.5.
5. Прочитай `PROGRESS.md` целиком. Найди: первый доступный шаг `todo` **для твоей машины (или ANY)**, а также любой `doing`/`reopened`/`blocked`.
6. **Спот-чек доверия (учитывает машины):** возьми два последних `done` шага, у которых машина = твоя или `ANY`, и реально выполни их команды приёмки. Для `done`-шагов чужой машины проверь только наличие коммита и артефактов (путь+хэш из журнала) — их команды ты выполнить не можешь. Если приёмка падает — поставь шагу `reopened` с причиной и почини (приоритетнее нового шага).
7. Если на **твоей машине** уже есть шаг `doing` с записью `ПЕРЕДАЧА` — продолжай его, а не бери новый.
8. На новой машине заполни «Окружение» в `PROGRESS.md` (GPU, драйвер, ОС, версии Rust/Windows SDK/DXC; шаги 0.1–0.2, 0.6 дают данные).
9. Продолжай цикл Ч.0.2.

### Ч.0.2. Цикл работы над ОДНИМ шагом

```
READ    → прочитай раздел шага + код/файлы, перечисленные в шаге
DO      → минимальное изменение ТОЛЬКО в «Файлах шага»
SCOPE   → `git diff --stat`: изменены только «Файлы шага» + PROGRESS.md (+ docs/legacy/*, тесты шага).
          Лишний файл → откати либо обоснуй в «Решениях». (Это проверка изоляции диффа НА КАЖДОМ шаге.)
VERIFY  → выполни КАЖДУЮ команду приёмки; вывод сохрани (цитата в журнал)
RECORD  → перечитай PROGRESS.md, обнови строку шага и добавь событие в «Последние события»
COMMIT  → один шаг = один коммит (формат Ч.0.5)
NEXT    → следующий шаг; если шаг нельзя завершить — Ч.0.4 / Ч.8
```

Правила цикла:

- Статусов шесть: `todo`, `doing`, `done`, `blocked`, `deferred`, `reopened`.
- **На каждую машину — не более одного шага в `doing`** (по колонке «Машина»; шаги `ANY` считаются за машину агента, который их взял; в примечании пиши «взял: DEV/TGT»). Два агента на одной машине одновременно запрещены. Параллельная работа на разных машинах допускается, если шаги независимы (зависимости соблюдать всегда).
- `done` ставится ТОЛЬКО если выполнены все пункты приёмки и их вывод записан в журнал.
- Приёмка упала 3 раза с разными попытками — `blocked`, причина, что нужно от владельца; остановись и отчитайся.
- Не перепрыгивай шаги: у каждого есть «Зависит». Зависимость не `done` — шаг не начинать.
- «(опционально)» — шаг можно оставить `deferred` с записью в журнале.
- **Ворота (G0–G4)** — обязательные точки отчёта владельцу (Ч.6). Для G2 и G4 нужно явное подтверждение владельца в журнале (раздел «Подтверждения владельца»); пока его нет — ворота не пройдены.

### Ч.0.3. Контекст ограничен (лимит агента)

Если контекст/лимиты подходят к концу — **немедленно** приведи систему в передаваемое состояние (Ч.0.4). Новый агент обязан суметь продолжить с того же места, не спрашивая владельца об истории.

### Ч.0.4. Чек-лист передачи сессии (при остановке, лимите, блокировке)

1. Заверши полуфабрикаты: `cargo build --workspace` зелёный, запущенные тесты не в сломанном состоянии. Иначе — `git checkout -- <файлы>` либо явно опиши незавершённое в журнале и верни шаг `doing`→`todo` с примечанием.
2. Обнови `PROGRESS.md`: статус шага (что сделано/нет), события, блокеры, «Next».
3. Закоммить по Ч.0.5 (допускается `WIP`-коммит со статусом `todo` — только если код компилируется). Если есть remote — `git push`.
4. Последней строкой в «Последние события»: `ПЕРЕДАЧА [машина]: следующему агенту начать с шага X.Y; главное: ...`.

### Ч.0.5. Коммиты — формат

`AGENTS.md` требует: тема `<Область>: <что сделано>` (~72 символа), области `Recompiler`, `D3D12.X`, `DirectStorage`, `Config`, `Docs`, `Tests`; без `fix:`/`feat(x):`. Требование «коммит только по просьбе» выполнено: **владелец настоящим плейбуком просит коммит после каждого завершённого шага** (постоянная инструкция).

```
<Область>: <краткое что сделано>

Step: <ID шага> (AGENT_PLAYBOOK.md)
Journal: PROGRESS.md обновлён
AI-assisted: <модель/агент, напр. Claude Sonnet 4.5>
```

- Артефакты, которые нельзя коммитить (дампы игры, скриншоты, большие логи): в репо — только код/документы; артефакты в каталоге вне репо (см. «Окружение»), в журнал — путь + размер + хэш.
- Запрещено коммитить: данные игры, ключи/токены, правки `HANDOFF.md`.

### Ч.0.6. ЖЕЛЕЗНЫЕ ПРАВИЛА (нарушение = брак шага)

1. **Ветка `legacy-gpu-backend`.** На `main` не работать.
2. **Не редактировать** `HANDOFF.md`, `docs/specs/*`, сгенерированные файлы (`*_spec.rs`, `tiling_tables.rs` и т.п. — через генераторы в `scripts/`).
3. **Никогда не запускать `cargo fmt`**; форматируй только свои строки.
4. **«Одно логическое изменение ≤ ~400 строк»** (AGENTS.md). Крупный шаг дели на `a/b/c` с отдельными коммитами.
5. **Mesh-путь не изменять.** Разрешённые файлы для всего проекта: `config.rs`, `preflight.rs`, `emit.rs` (только НОВЫЕ функции), `pso.rs`, `real.rs`, `bind.rs`, `frame.rs`, `report.rs`, `gpu/recomp/legacy/*`, новые тесты/docs, `tools/*`. Правки существующих mesh-функций (`mesh_passthrough*`, `mesh_dispatch.rs`, `mesh_wave.rs`, существующие тесты и `runner.rs` вне добавленных функций) — только через запись в «Решения» с обоснованием и минимальным диффом.
6. GPU-тесты: при отсутствии железа вызывай `runner::skip_test(reason)` и выходи; `#[ignore]` запрещён; CI включает их через `XCL_REQUIRE_GPU=wave64|mesh`.
7. **Приёмка только реальными командами.** Цитируй вывод в `PROGRESS.md`. Числа — из `report.json`/логов/команд, не из памяти.
8. Ошибка в коде → тест, который без неё падает (AGENTS.md); юнит-тесты — в `#[cfg(test)] mod tests` рядом с кодом.
9. Каждое проектное решение — в «Решения» (append-only) и в `docs/legacy/contract.md`.
10. Не запускать разрушительного вне репозитория; не трогать файлы игры (только чтение дампов).
11. Креденшелы/токены не печатать и не сохранять.
12. macOS/Linux не поддерживаются: сборка и тесты только на Windows 10/11 x64 (MSVC). WSL — нет.

### Ч.0.7. Машины

| Код | Машина | Что известно |
|---|---|---|
| **DEV** | ПК с mesh-GPU (AMD RDNA4), где разрабатывается XCL | mesh shaders есть; эталон для golden-сравнений |
| **TGT** | ПК с GTX 1080 (Pascal) | mesh shaders **нет**; wave32; целевая машина |
| **ANY** | любая | CPU-only шаги |

`TGT`-шаги требуют физического доступа к 1080. Нет такой машины — шаг остаётся `todo` с примечанием «ждёт TGT».

### Ч.0.8. Инструменты и пути XCL (справка)

- Сборка: `cargo build --release --workspace` → `target\release\xcl.exe`. Полный тест: `.\scripts\test.ps1` (= `cargo build --workspace && cargo test --workspace -- --test-threads=4`). Один район: `cargo test -p xcl --lib <фильтр>`.
- Запуск игры: `xcl run D:\Game [-- аргументы]`; инспекция: `xcl inspect D:\Game\game.exe`; precompile шейдеров: `xcl shaders D:\data\shaders.rpf --precompile --only cs --pipelines`.
- Конфиг per-title: `%LOCALAPPDATA%\XCL\<title>\xcl.toml`; логи: `...\logs\xcl.log`, `report.json` (предыдущий прогон: `*.prev.*`).
- Переменные: `XCL_ADAPTER` (в т.ч. `warp`), `XCL_DXC_DIR`, `XCL_D3D12_DIR`, `XCL_SAMPLE_RPF`, `XCL_REQUIRE_GPU=wave64|mesh`, `XCL_PSO_TRACE=1`, `XCL_DRED=1`, `XCL_FRAME_DUMP=N` (пиксельная статистика кадра → `report.json` → `d3d12.last_frame_dump`), `XCL_DIRECT_DXIL=1`.
- Smoke-регрессия: `.\scripts\smoke.ps1 -Game <folder>` → блок `xcl-smoke`.
- Среда: Windows SDK (DXC: `dxcompiler.dll`+`dxil.dll`), Agility SDK `D3D12Core.dll` рядом с `xcl.exe` или в `%XCL_D3D12_DIR%`, системный `xgameruntime.dll`, Rust `stable-x86_64-pc-windows-msvc`. DXC 1.8+ рекомендуется.

---

## Ч.1. Контекст: что за проект и в чём задача

XCL запускает игры Xbox Series X|S на PC **без CPU-эмуляции**: код игры — нативный x64, слой подставляет библиотеки (`kernelx`, XGameRuntime, D3D12.X, XAudio2/XMA2, GameInput) и перекомпилирует RDNA2-шейдеры консоли в DXIL через DXC. Требование XCL — GPU с Shader Model 6.6 **и mesh shaders**. На GTX 1080 mesh shaders нет, поэтому консольные программы геометрии (NGG / MS-стадия, а также GS-связки) на ней ничего не рисуют.

**Как это устроено в коде сейчас (разведка, коммит `ecf1376`):**

- Консольная NGG/MS-программа компилируется **в два прохода**: первый — вся программа как `cs_6_6` (`Mode::Mesh` → compute), пишет **по одной записи (record) на thread group** в UAV-буфер `res` (root u1); формат записи: заголовок 4 dword `[vertex_count, primitive_count, 2 spare]` + `max_vertices` × (pos float4 + по float4 на каждый ranked output) + `max_primitives` × 1 dword (упаковка 10-10-10-1). См. `recomp/mod.rs`: `MESH_HEADER`, `vert_dwords()`, `record_dwords()`, `MsInterface` (~158–167).
- Второй проход — passthrough mesh shader (`emit.rs`: `mesh_passthrough()` ~5209, `mesh_passthrough_for()` ~5220, split-режим `passthrough_split()` ~5333 из-за лимита 32 КБ выходов mesh) + `DispatchMesh`.
- PSO создаётся в `hle/d3d12x/real.rs`: `finish_two_pass()` ~5296, `two_pass_mesh_stream()` ~5215, реестр `TWO_PASS` ~5117, `TwoPassPso` ~5097. Классический путь **VS+PS**: `translated_graphics_pso()` ~4595 → `TRANSLATED_GRAPHICS`.
- **Runtime draw-путь для двухпроходных PSO в форке НЕ реализован** (TODO в `HANDOFF.md` ~1227). Единственная рабочая последовательность — тестовый бенч `gpu/recomp/runner.rs`: `mesh_draw()` ~1271 / `mesh_draw_split()` ~1287–1450 (Dispatch → `uav_barrier(res)` → SetPipelineState(mesh PSO) → `DispatchMesh(groups*split,1,1)`).
- ABI-биндинг: root signature `pso.rs::abi_root_signature_with()` ~1046 (6 root UAV: u0 io, u1 res, u2–u5 окна памяти UMA; heap-directly-indexed), при дроу/диспаче — `bind.rs::bind_abi()` ~945.
- Хуки дроу: `bind.rs::draw_instanced` ~444, `draw_indexed_instanced` ~461, `prepare_indexed_draw` ~469, `graphics_bound` ~495, `emit_prepared_indexed_draw` ~481, `draw_rects` ~384 (пример «не-mesh» пути: rect-list GS → `DrawInstanced(rects*6)`), `execute_indirect` ~253 (compute indirect; graphics indirect не переведён), `dispatch` ~803.
- Фича-детект: `preflight.rs::probe_gpu()` ~103–152 → `Gpu { name, warp, shader_model, no_sm66, mesh, wave_min, wave_max }`. **SM 6.6 — жёсткое требование** (agility.rs `format_no_sm66_message`); на Pascal проверяется в этапе 0.
- wave64 на wave32: консольные wave64-программы на 32-поточных GPU исполняются двумя полуволнами (`docs/wave64-on-wave32.md`; `mesh_dispatch.rs`, `mesh_wave.rs`; `pso.rs::wave_range()` ~959). Для compute-проходов уже реализовано — легаси-бэкенд наследует.
- Конфиг: `config.rs` (`Config` ~10, `load_or_create` ~207, образец парсера — `raytracing_tier_override()` ~251–267).
- Отчёт/счётчики: `report.rs` (`Report` ~44, `D3d12Stats` в `real.rs` ~409), `bind.rs::count()/counts_snapshot()` ~45–64.

**Суть легаси-бэкенда.** Вместо mesh-прохода после основного compute-прохода (который пишет записи `res`) выполняется цепочка:

```
[основной compute-проход NGG → res (записи)]            (уже существует)
   → scan-CS:  префиксная сумма primitive_count по записям (детерминированные оффсеты) + args
   → recon-CS: по записям раскладывает индексы примитивов в IB (глобальные id вершин)
   → [cull-CS: опционально, стабильная компакция IB → IB2 + args]
   → ExecuteIndirect(DRAW) обычным graphics-PSO: passthrough VS (fetch из res по IB) + консольный PS
```

Вершины по умолчанию **остаются в `res`** (вариант «без VB», D3-A); VB-копия — запасной вариант D3-B.

---

## Ч.2. Карта кода (файл → роль → что меняется)

| Файл (`crates/xcl/src/...`) | Роль | Легаси-бэкенд |
|---|---|---|
| `gpu/recomp/mod.rs` | `translate()` ~1386, enum `Mode` ~272, `MsInterface`/record-формат | читать; возможно расширить Options |
| `gpu/recomp/pso.rs` | `console_mesh()` ~929, `compile_console_mesh()` ~852, `finish_two_pass_compiled()` ~836, `abi_root_signature_with()` ~1046, `wave_range()` ~959 | ветвление legacy при сборке TwoPass; legacy root signature |
| `gpu/recomp/emit.rs` | эмиттер HLSL; `mesh_passthrough_for()` ~5220, `passthrough_split()` ~5333, `rect_list_gs()` ~5170 | НОВЫЕ эмиттеры: scan-CS, recon-CS, passthrough VS (только новые функции) |
| `gpu/recomp/runner.rs` | GPU-бенч: `mesh_draw_split()` ~1287, `dispatch()` ~1605, `skip_test()` ~617, `adapter()` ~532 | golden-тесты legacy (по образцу mesh_draw) |
| `gpu/recomp/mesh_dispatch.rs`, `mesh_wave.rs` | wave64↔wave32 rendezvous | не менять |
| **`gpu/recomp/legacy/`** | **новый модуль**: `mod.rs`, `reconstruction.rs`, `vbib.rs`, `cull.rs`, `indirect.rs` | основная новая работа |
| `hle/d3d12x/real.rs` | PSO-хуки: `finish_two_pass()`, `translated_graphics_pso()`, `TWO_PASS`, `TRANSLATED_GRAPHICS`, `dev_create_command_signature()` ~3237, `D3d12Stats` ~409 | legacy-PSO, CommandSignature, счётчики |
| `hle/d3d12x/bind.rs` | `bind_abi()` ~945, `graphics_bound()` ~495, `prepare_indexed_draw()` ~469, `execute_indirect()` ~253, `count()` ~45 | ветвление legacy при дроу, цепочка |
| `hle/d3d12x/list_x.rs` | консольные слоты: `dispatch_x` ~2767, `draw_indexed_x` ~3630 | как правило, не менять |
| `hle/d3d12x/frame.rs` | кадр/present | сброс аллокатора на кадр; сохранение RT (4.5) |
| `config.rs` | per-title TOML | `legacy_backend`, `cull_*`, `legacy_budget_mb` |
| `preflight.rs` | `probe_gpu()` ~103 | legacy-флаг в отчёт |
| `report.rs` | `Report` ~44 | поля legacy |

Тесты-образцы: `gpu_mesh_mode_two_passes` (`recomp/tests.rs` ~3333), `mesh_split_accounts_vertex_chunks` ~3432, `passthrough_mesh_stream_accepts_a_triangle_driver_blob` (`real.rs` ~5225), `retail_mesh_pipelines_through_the_hook` (`crates/xcl/tests/gpu_pso_e2e.rs` ~608), `retail_gs_pipeline_through_the_hook` ~680.

---

## Ч.3. Журнал `PROGRESS.md` — формат и правила

- Шаблон: `PROGRESS_TEMPLATE.md`. Первым делом скопировать в `PROGRESS.md`.
- Разделы: **Окружение** (DEV/TGT), **Статус шагов** (Шаг | Название | Статус | Машина | Дата | Коммит | Артефакты | Примечание), **Контрольные ворота**, **Подтверждения владельца**, **Последние события** (append-only: `[дата, машина, агент] что произошло`), **Решения** (append-only, D1..Dn), **Блокеры**, **Next**.
- Перед каждой записью — перечитать файл целиком (и сделать `git pull --rebase`, если есть remote).
- «Последние события», «Решения», «Подтверждения владельца» — только добавление строк.
- В примечании шага `done` обязательно: какие команды приёмки выполнены + краткий вывод/цифры.
- Артефакты вне репо: путь + хэш.

## Ч.4. Проектные решения (дефолты; зафиксировать в `docs/legacy/contract.md` на шаге 1.6 и продублировать в журнале)

| ID | Вопрос | Решение по умолчанию |
|---|---|---|
| D1 | Форма дроу | **Fetch в VS**: неиндексированный `Draw`, VS по `SV_VertexID` читает индекс из IB (root SRV), затем вершину (D3). Причина: не трогаем IA-состояние тайтла. Альтернатива (indexed+IA) — только если D1 заблокирует |
| D2 | Реконструкция и **порядок примитивов** | **Детерминированный порядок = (индекс записи, слот примитива).** Без атомарного append: scan-CS считает эксклюзивную префиксную сумму `primitive_count` по записям (из заголовков записей `res`, они уже содержат счётчики), recon-CS пишет индексы в `IB[(prefix[rec]+slot)*3 + k]`. 1 группа = 1 запись (×split), поток = слот примитива, поток за пределами `primitive_count` выходит. Scan — на group-shared памяти, **без wave-интринсиков** (независим от wave32/64). Порядок критичен для blending/декалей/z-fight, а не только для тестов |
| D3 | Хранилище вершин | **D3-A (по умолчанию): «без VB».** Вершины остаются в записях `res`; IB содержит **глобальный id вершины** `rec*max_vertices + local`; VS по id вычисляет адрес в `res` (`rec = id / max_vertices`). Копирования/переупаковки нет, трафик памяти минимален. Для graphics-стадии `res` читается как root SRV (переход `UAV→NON_PIXEL_SHADER_RESOURCE`). **D3-B (запасной): VB-копия** с компакцией вершин (второй scan по `vertex_count`). Окончательный выбор — шаг 1.5 (анализ) и, если нужно, 7.5 (замер) |
| D4 | Формат вершины | `float4 position` + по `float4` на каждый ranked output — как в `res` (D3-A) или в VB (D3-B); порядок ranked сохраняется |
| D5 | Culling | **Opt-in, `cull_enable` default `false`** до прохождения G3 и замера 7.4. Frustum-тест без внешних параметров: в clip-space примитив отсекается, только если **все три вершины** вне одной и той же плоскости (`x<-w`, `x>w`, `y<-w`, `y>w`, `z<0`, `z>w`; при `w<=0` — только если все вершины за той же плоскостью). Любой сомнительный случай → **не отсекать** (ложное отсечение хуже отсутствия оптимизации). Компакция **стабильная** (порядок сохраняется). Backface — флаг (default off; решение по результату аудита 1.3). Hi-Z — deferred |
| D6 | Indirect | `ExecuteIndirect` с signature типа DRAW (16 байт: VertexCountPerInstance, InstanceCount, StartVertex, StartInstance). Args пишет **scan-CS (финализация)**, а при включённом cull — cull-CS. Indirect используется с первого runtime-draw (временных «draw по полному capacity» нет) |
| D7 | Атрибуты примитива | Порядок вершин тройки сохраняется как в упакованном dword (v0,v1,v2); **provoking vertex = v0** (D3D-конвенция) — проверить на 1.3. Flat-атрибуты берутся по provoking. Если у программы есть **per-primitive outputs** или нужен `SV_PrimitiveID`: после компакции позиция в IB ≠ исходный слот → добавляется параллельный буфер `prim_src[(rec,slot)]` (пишет recon-CS, переупорядочивает cull-CS). Cull/clip distance — переносятся как обычные outputs, наличие фиксируется в 1.3 |
| D8 | Ёмкость и аллокатор | Регионы под draw выделяются **bump-аллокатором из per-frame кольца (N=3 кадра в полёте)** во время записи draw; размеры известны точно: `records = groups*split` → IB = `3 * records * max_primitives * 4` (или u16-вариант), scan-буферы, args. Сброс аллокатора — в начале кадра (frame.rs). Бюджет `legacy_budget_mb` (default 1024); превышение → draw пропускается + `LEGACY_OVERFLOW` + `xlog_warn!` (не «молча»). Оценка пиковой ёмкости по программам тайтла — шаг 1.5 (до реализации) |

---

## Ч.5. ШАГИ

Обозначения: **Машина** — где выполняется; **Зависит** — обязательные предки; **Файлы шага** — что разрешено менять (Ч.0.2 SCOPE); **Приёмка** — все пункты обязательны.

### Этап 0 — Разведка: доказать возможности GTX 1080, зафиксировать baseline и ранние риски

#### 0.1 Проба-инструмент на TGT: устройство и инвентарь
- **Машина:** TGT. **Зависит:** —. **Файлы шага:** `tools/legacy_probe/*`.
- **Цель:** фактические данные о GTX 1080 до любых изменений кода.
- **Что делать:**
  1. Создать standalone-проект `tools/legacy_probe/` (вне workspace; свой `Cargo.toml`, dep `windows` 0.62, фичи `Win32_Graphics_Direct3D12`, `Win32_Graphics_Dxgi`, `Win32_Graphics_Direct3D`). Проект коммитится, `target/` — нет.
  2. Перечислить адаптеры (`IDXGIFactory1::EnumAdapters1`), создать `ID3D12Device` (максимальный FL ≥ 12_0), напечатать: имя, description, dedicated/shared memory, LUID, версию драйвера (реестр/wmic или «н/д»).
- **Приёмка:**
  1. `cargo run --manifest-path tools/legacy_probe/Cargo.toml` → имя адаптера и «device created», без паник.
  2. Вывод процитирован в «Окружение» TGT.

#### 0.2 Проба-инструмент: фичи D3D12 (mesh tier, wave lanes, SM)
- **Машина:** TGT. **Зависит:** 0.1. **Файлы шага:** `tools/legacy_probe/*`.
- **Что делать:** добавить запросы (образец — `preflight.rs::probe_gpu()`): `OPTIONS7` (`MeshShaderTier`, `SamplerFeedbackTier`); `OPTIONS1` (`WaveLaneCountMin/Max`, ожидаемо 32); SM-проба `6_6/6_5/6_0` (максимальная поддержанная); `OPTIONS/OPTIONS3/OPTIONS12/OPTIONS14` (binding tier, tiled resources и т.д.). Результат — `tools/legacy_probe/out_tgt.json` (serde_json) + stdout.
- **Приёмка:** `out_tgt.json` создан и закоммичен; в журнале ключевые значения (MeshShaderTier, WaveLaneCountMax, макс. SM). Если макс. SM < 6.6 — немедленно отметить как главный риск (G0).

#### 0.3 GATE G0: SM 6.6 PSO на TGT
- **Машина:** TGT. **Зависит:** 0.2. **Файлы шага:** `tools/legacy_probe/*`.
- **Цель:** работает ли DXIL SM 6.6 на Pascal (XCL требует 6.6 везде).
- **Что делать:**
  1. Найти DXC (`%XCL_DXC_DIR%` или Windows SDK; образец — `gpu/recomp/dxc.rs::location()`).
  2. Мини-HLSL с `ResourceDescriptorHeap` → `cs_6_6` через `DxcCreateInstance`, подпись `dxil.dll`, `CreateComputePipelineState`.
  3. Повтор для SM 6.0 с wave-интринсиками (`WaveActiveSum`).
  4. `out_tgt.json`: `sm66_pso: ok|fail(+HRESULT)`, `sm60_wave_pso: ok|fail`.
- **Приёмка:** оба теста выполнены, результат в JSON и журнале. **Если `sm66_pso = fail` → СТОП: этапы 2–8 не начинать**, G0=fail, отчёт владельцу (fallback — решение владельца).
- **Записать:** вердикт G0 в «Решения» и «Контрольные ворота».

#### 0.4 Baseline на DEV
- **Машина:** DEV. **Зависит:** — (параллельно 0.1–0.3). **Файлы шага:** — (допустим `0.4a` на починку сборки main).
- **Что делать:** `cargo build --release --workspace`; `.\scripts\test.ps1` (passed/failed/skipped); запуск тестового/целевого тайтла, `.\scripts\smoke.ps1 -Game <folder>`; копии `report.json`, `xcl.log` в каталог артефактов; зафиксировать, что графического работает сейчас (ожидаемо — runtime mesh-путь не подключён, Ч.1).
- **Приёмка:** `git status` чист; артефакты существуют (путь+хэш); в журнале числа тестов и вывод smoke.

#### 0.5 Ветка и плейбук в репозитории
- **Машина:** ANY. **Зависит:** 0.4. **Файлы шага:** `AGENT_PLAYBOOK.md`, `PROGRESS.md`, `PROGRESS_TEMPLATE.md`.
- **Что делать:** `git switch -c legacy-gpu-backend`; скопировать `AGENT_PLAYBOOK.md`, `PROGRESS_TEMPLATE.md`; `PROGRESS_TEMPLATE.md` → `PROGRESS.md` (заполнить). Коммит: `Docs: add legacy backend playbook and progress journal`. Если есть remote — `git push -u`.
- **Приёмка:** `git branch --show-current` = `legacy-gpu-backend`; файлы в корне; коммит в `git log`.

#### 0.6 TGT: сборка XCL и ранний прогон compute-тестов (**ранний риск**)
- **Машина:** TGT. **Зависит:** 0.5, 0.3 (G0 ok). **Файлы шага:** — (только `PROGRESS.md`; правки кода — отдельным коммитом с обоснованием).
- **Цель:** с самого начала знать, как XCL и compute-проход NGG ведут себя на реальном Pascal (wave32), а не узнать это на этапе 7.
- **Что делать:**
  1. Подготовить среду (Rust, Windows SDK/DXC 1.8+, Agility `D3D12Core.dll`, `xgameruntime.dll`), `cargo build --release --workspace`.
  2. `cargo test -p xcl --lib mesh` и `cargo test -p xcl --lib recomp` (или ближайшие фильтры); для **каждого skipped** теста записать причину. Составить список тестов, которые исполняют **только compute-проход** (напр. `gpu_mesh_mode_two_passes`-семейство) и должны выполняться без mesh tier; если они скипаются по условию «нужен mesh» — завести пометку для 7.3 (не править сейчас).
  3. `xcl inspect` на тайтл (если доступен).
- **Приёмка:** сборка зелёная (вывод в журнал); таблица «тест → passed/failed/skipped(причина)» в журнале; wave-размер адаптера (32) подтверждён логом/отчётом.
- **Записать:** версии инструментов, таблицу. Любой **failed** compute-тест на Pascal — главный сигнал: вынести в «Блокеры» и сообщить владельцу до этапа 3.

#### 0.7 DEV: прогон compute-прохода на WARP (wave32)
- **Машина:** DEV. **Зависит:** 0.4. **Файлы шага:** —.
- **Что делать:** `$env:XCL_ADAPTER="warp"; cargo test -p xcl --lib mesh` (и `recomp`): записать passed/failed/skipped и причины. Цель — раннее подтверждение, что рандеву wave64→wave32 основного compute-прохода работает на 32-поточном адаптере.
- **Приёмка:** таблица результатов в журнале; различия с DEV-нативным прогоном 0.4 объяснены.

### Этап 1 — Аудит и дизайн: зафиксировать контракт данных (read-only + документы)

**Общее требование к документам аудита (1.1–1.4):** вместо «≥ N ссылок» — **таблица утверждений**: `# | утверждение | ссылка файл:символ | как проверено (прочитан код / тест / лог запуска)`. Каждое утверждение, определяющее формат данных или порядок, должно быть проверено **запуском или тестом**, где это возможно (напр. дамп реальной записи `res` из теста), а не только чтением. Следующий агент при спот-чеке перепроверяет 3 случайных утверждения.

#### 1.1 Аудит PSO/record-формата → `docs/legacy/audit-pso.md`
- **Машина:** ANY. **Зависит:** 0.5. **Файлы шага:** `docs/legacy/audit-pso.md`.
- **Что делать:** изучить `pso.rs` (`console_mesh` ~929, `compile_console_mesh` ~852, `finish_two_pass_compiled` ~836, `abi_root_signature_with` ~1046, `TwoPass`, `MsInterface`) и `recomp/mod.rs` (`MESH_HEADER`, `vert_dwords`, `record_dwords`, ranked outputs). Зафиксировать: (а) как считается `mesh_groups_per_record` и split; (б) layout записи dword-за-dword; (в) root-параметры прохода; (г) где `Mode::Mesh` компилируется в `cs_6_6`.
- **Приёмка:** таблица утверждений; layout записи с формулами из кода (`MESH_HEADER + max_vertices*vert_dwords + max_primitives`); минимум одно утверждение о layout подтверждено дампом записи из реального теста.

#### 1.2 Аудит two-pass-исполнения и split → `docs/legacy/audit-dispatch.md`
- **Машина:** ANY. **Зависит:** 1.1. **Файлы шага:** `docs/legacy/audit-dispatch.md`.
- **Что делать:** `emit.rs::mesh_passthrough_for()` ~5220, `passthrough_split()` ~5333 (лимит 32 КБ: `passthrough_max_vertices`/`passthrough_elements`), эталон `runner.rs::mesh_draw_split()` ~1287–1450. Зафиксировать: (а) что получает/отдаёт mesh-проход; (б) как split меняет dispatch-математику; (в) точную последовательность D3D12-вызовов эталона построчно.
- **Приёмка:** документ с таблицей утверждений; последовательность вызовов эталона выписана с комментариями.

#### 1.3 Аудит выходов NGG-программ, атрибутов примитива и связи с PS → `docs/legacy/audit-emit.md`
- **Машина:** ANY. **Зависит:** 1.1. **Файлы шага:** `docs/legacy/audit-emit.md`.
- **Что делать:** `ranked()`, состав `vert_dwords`, как passthrough MS подгоняется под вход PS (`mesh_passthrough_for(ms, ps_inputs)`, clip/cull «kept»). Ответить письменно:
  (а) что PS ожидает на входе (имена/семантики интерполяторов);
  (б) делает ли NGG-программа консоли сама backface/clip-cull (решает D5/5.2);
  (в) как формируется примитивный dword 10-10-10-1 и как из него достаются 3 индекса; порядок вершин и **provoking vertex** (D7);
  (г) есть ли **flat-интерполяция, per-primitive outputs, `SV_PrimitiveID`, cull/clip distance** (D7); если есть — как они лежат в записи;
  (д) как mesh-путь обрабатывает вырожденные примитивы.
- **Приёмка:** таблица «output → где живёт в записи → что ожидает PS»; явный вердикт по (б), (г) со ссылками на код.
- **Записать:** вердикты D5 и D7 в «Решения».

#### 1.4 Аудит стадийных комбинаций и GS → `docs/legacy/audit-stages.md`
- **Машина:** DEV. **Зависит:** 1.1. **Файлы шага:** `docs/legacy/audit-stages.md`.
- **Что делать:** `pso.rs::console_geometry()` ~982, `real.rs::translated_gs_pso()` ~5455, `rect_list_gs()` ~5170; по дампу тайтла (`xcl shaders ... --precompile` / `XCL_SAMPLE_RPF`) выяснить реальные связки: MS(NGG) без GS; ES+GS; VS+PS. Для NGG-программ собрать статистику: `max_vertices`, `max_primitives`, `record_dwords`, наличие per-primitive outputs. Нет дампа — «по корпусу GS существуют», отложить до 7.x (не блокировать).
- **Приёмка:** список комбинаций с долями (числа, если дамп доступен); таблица параметров NGG-программ (вход для 1.5); вывод «что обязан поддерживать legacy-путь».

#### 1.5 Дизайн-решения и оценка ёмкости → `docs/legacy/design-decisions.md`
- **Машина:** ANY (данные — DEV). **Зависит:** 1.3, 1.4. **Файлы шага:** `docs/legacy/design-decisions.md`.
- **Цель:** принять до написания кода три решения, ошибка в которых дорого стоит: порядок (D2), «без VB» vs VB (D3), ёмкость (D8).
- **Что делать:**
  1. **D2:** описать scan-схему (блочный scan по 1024 записей на группу + scan сумм блоков одной группой, поддержка ≤ 1M записей), доказать детерминизм порядка; оценить число диспатчей (scan: 2–3, recon: 1).
  2. **D3:** сравнить D3-A (вершины в `res`) и D3-B (VB-копия): трафик памяти (байты чтения/записи на draw по формулам), занятая память, влияние разреженности `res` на кэш (доля заполнения записи = `vertex_count/max_vertices`, по статистике 1.4), сложность VS. Выбрать, сохранив D3-A, если доля заполнения не катастрофична (< ~15% — повод пересмотреть); зафиксировать пороги для переоценки на 7.5.
  3. **D8:** по статистике 1.4 посчитать ёмкость IB/scan/args на типичные и **максимальные** draw (МБ) и пик за кадр при оценке «N draw»; если данных нет — формулы + параметры для замера на 4.4 (логирование реальных пиков). Предложить значение `legacy_budget_mb`.
  4. Описать D7-схему (provoking, `prim_src`) по результатам 1.3.
- **Приёмка:** документ с таблицей решений D2/D3/D7/D8, числами и порогами пересмотра; все допущения помечены «проверить на шаге X».
- **Записать:** итоговый выбор в «Решения».

#### 1.6 GATE-документ: `docs/legacy/contract.md` (ТЗ всего плана)
- **Машина:** ANY. **Зависит:** 1.1–1.5. **Файлы шага:** `docs/legacy/contract.md`.
- **Что делать:** свести в один документ: compute-проход → записи `res` → что обязан произвести legacy-бэкенд (IB/scan/args[/VB]) → что ожидает PS; root-параметры (порядок фиксируется здесь); состояния ресурсов и барьеры; решения D1–D8 с обоснованием; вопросы аудита.
- **Приёмка:** диаграмма pass-цепочки текстом; таблица D1–D8; все вопросы либо закрыты, либо в «Блокерах».
- **Записать:** ссылку; попросить владельца просмотреть (не блокируя — работаем на дефолтах).

#### 1.7 Карта точек вставки runtime → `docs/legacy/audit-runtime.md`
- **Машина:** ANY. **Зависит:** 1.2. **Файлы шага:** `docs/legacy/audit-runtime.md`.
- **Что делать:** выписать точки вставки legacy-ветки: `real.rs::finish_two_pass()`, `translated_mesh_stream_once()`/`translated_gs_pso()`, `bind.rs::graphics_bound()`, `prepare_indexed_draw()`/`draw_instanced`/`execute_indirect()`, `real.rs::dev_create_command_signature()` ~3237, `frame.rs` (начало кадра/present). Для каждой — что проверяется сейчас и где в процессе записи command list известен `groups*split`.
- **Приёмка:** документ; каждая точка с `файл:символ`, 1-строчным описанием и таблицей утверждений.

### Этап 2 — Каркас legacy-режима (безопасный пустой режим)

#### 2.1 Конфиг
- **Машина:** ANY. **Зависит:** 1.6. **Файлы шага:** `config.rs`.
- **Что делать:** `legacy_backend: String` (default `"auto"`; `auto|on|off`), парсер `legacy_backend_mode()` (образец — `raytracing_tier_override()` ~251–267) + юнит-тесты (парсинг, дефолт, round-trip toml). `auto` = legacy на GPU без mesh tier. Также: `cull_enable: bool` (**default false**, D5), `cull_backface: bool` (default false), `legacy_budget_mb: u32` (default 1024, D8) — пока только хранить.
- **Приёмка:** `cargo test -p xcl --lib config` зелёный; тест round-trip существует.

#### 2.2 Детект и `effective_legacy()`
- **Машина:** ANY. **Зависит:** 2.1. **Файлы шага:** `preflight.rs` и/или `gpu/recomp/legacy/mod.rs`.
- **Что делать:** `effective_legacy(cfg_mode, gpu:&Gpu) -> (bool, &'static str)`: `on`→true («forced»), `off`→false («disabled»), `auto`→`!gpu.mesh`. Тесты: 3 режима × 2 GPU. Лог при старте: `xlog_info!("legacy backend: {режим} ({причина})")`.
- **Приёмка:** `cargo test -p xcl --lib legacy` зелёный; строка видна в логах при запуске `xcl run` (на ANY-машине — в юнит-тесте).

#### 2.3 Проброс флага в PSO-хуки (log-only)
- **Машина:** DEV. **Зависит:** 2.2. **Файлы шага:** `real.rs`, `bind.rs`.
- **Что делать:** в `finish_two_pass()` ~5296 и `translated_mesh_stream_once()` ~5379: при `effective_legacy == true` mesh-PSO не создавать, логировать, `count("PSO_LEGACY_PENDING")`, регистрировать с kind=LegacyPending в `TWO_PASS`; дроу на таких PSO скипается с логом.
- **Приёмка:**
  1. `cargo build --workspace` и `cargo test -p xcl --lib two_pass` зелёные.
  2. DEV, testtitle: `legacy_backend="on"` → в `xcl.log` `PSO_LEGACY_PENDING`, нет паник; `"off"` → лог идентичен baseline 0.4 (`diff`: отличия только в legacy-строках).

#### 2.4 Smoke «legacy off == baseline»
- **Машина:** DEV. **Зависит:** 2.3. **Файлы шага:** —.
- **Приёмка:** `.\scripts\smoke.ps1 -Game <folder>` при `off`; оба блока `xcl-smoke` (baseline и новый) вставлены в журнал; в цифрах различий нет.

### Этап 3 — Реконструкция: NGG-записи → индексы (D2/D3)

#### 3.1 Скелет модуля `gpu/recomp/legacy/`
- **Машина:** ANY. **Зависит:** 1.7. **Файлы шага:** `gpu/recomp/legacy/*`, `gpu/recomp/mod.rs` (только `mod legacy;`).
- **Что делать:** файлы `mod.rs`, `reconstruction.rs`, `vbib.rs`, `cull.rs`, `indirect.rs` (заготовки); в `mod.rs` — `GeometryStoreLayout` (поля по D3/D8: `ib_offset`, `scan_offset`, `args_offset`, `prim_src_offset?`, `vb_*` (если D3-B), `total_bytes`).
- **Приёмка:** `cargo build --workspace` и `cargo test -p xcl --lib legacy` зелёные.

#### 3.2 Layout, ёмкость и per-frame аллокатор (D8)
- **Машина:** ANY. **Зависит:** 3.1, 1.5. **Файлы шага:** `gpu/recomp/legacy/vbib.rs`, `mod.rs`.
- **Что делать:**
  1. `store_layout(record:&MsInterface, groups:u32, split:u32) -> GeometryStoreLayout` по выбранному в 1.5 варианту (D3-A: IB + scan + args [+ prim_src]; D3-B: + VB): ёмкости из формул 1.5, оффсеты выровнены на 256 байт.
  2. `StoreRing`: bump-аллокатор регионов по кольцу N=3 кадра; `begin_frame()` сбрасывает регион текущего кадра; бюджет `legacy_budget_mb`; при превышении — `Err(Overflow)` (вызывающий скипает draw и считает `LEGACY_OVERFLOW`); статистика пика за кадр.
- **Приёмка:** `cargo test -p xcl --lib vbib` — ≥ 8 тестов: (а) ёмкости совпадают с формулой 1.5; (б) нет переполнения u32/u64; (в) выравнивание 256 Б; (г) аллокатор: выделение, сброс кадра, переиспользование, overflow; (д) три кадра подряд не пересекаются по адресам.

#### 3.3 Эмиттер scan-CS (детерминированные оффсеты)
- **Машина:** ANY. **Зависит:** 3.2. **Файлы шага:** `emit.rs` (новая функция), `gpu/recomp/legacy/reconstruction.rs`.
- **Что делать:** `scan_cs(record:&MsInterface) -> String` (cs_6_6): читает `primitive_count` из заголовка каждой записи, считает эксклюзивный префикс (блочный scan по group-shared, затем добавление оффсетов блоков), пишет `prefix[rec]` в scan-буфер; **финализация** пишет args-блок (`VertexCountPerInstance = total_prims*3`, остальное 1/0/0; D6). **Запрещены wave-интринсики** (`WaveActive*`, `WaveGet*`, `WavePrefix*`).
- **Приёмка:**
  1. Юнит-тест: HLSL компилируется DXC в `cs_6_6` (без DXC — `skip_test`).
  2. Тест CPU-референса scan (на синтетических заголовках: 1, 1023, 1024, 1025, 1_000_000 записей) — тот же алгоритм в Rust, сравнение.
  3. `grep -E "Wave(Active|Get|Prefix|Read)"` по сгенерированному HLSL → 0 совпадений (вывод в журнал).

#### 3.4 Эмиттер recon-CS (индексы примитивов)
- **Машина:** ANY. **Зависит:** 3.3, 1.3. **Файлы шага:** `emit.rs` (новая функция), `gpu/recomp/legacy/reconstruction.rs`.
- **Что делать:** `reconstruction_cs(record:&MsInterface) -> String`: 1 группа = 1 запись (× split, тот же расчёт, что `passthrough_split`), поток = слот примитива; поток с `slot >= primitive_count` выходит; распаковка 10-10-10-1 → 3 локальных индекса; запись `IB[(prefix[rec]+slot)*3 + k] = rec*max_vertices + local` (D3-A). Порядок вершин тройки сохраняется (D7). Если по 1.3 нужны per-primitive outputs/`SV_PrimitiveID` — пишет `prim_src`. Если выбран D3-B — дополнительно копирует вершины (отдельный scan вершин; выделить в подшаг 3.4b).
- **Приёмка:** (1) HLSL компилируется в `cs_6_6`; (2) тест арифметики dispatch: групп recon == `records*split`; (3) grep wave-интринсиков = 0.

#### 3.5 Golden: legacy-CS == mesh-проход
- **Машина:** DEV. **Зависит:** 3.4. **Файлы шага:** `gpu/recomp/runner.rs` (только новые функции), `gpu/recomp/tests.rs` (новые тесты).
- **Что делать:** тест (образец — `gpu_mesh_mode_two_passes` ~3333) на синтетической NGG-программе: (1) основной compute-проход → records; CPU-референс строит ожидаемый IB из records; (2) scan + recon через `runner::dispatch()`; (3) прочитать IB, args и сравнить с референсом **побайтно, включая порядок** (порядок детерминирован, D2). Обязательные случаи: пустая запись (`primitive_count=0`), запись с `primitive_count=max_primitives`, много записей (>1024, чтобы задействовать межблочный scan), split>1.
- **Приёмка:** тест зелёный на DEV (без GPU — `skip_test`); 0 расхождений; повторный прогон 20 раз даёт тот же результат (детерминизм) — вывод в журнал.

#### 3.6 Корректность на wave32
- **Машина:** DEV (WARP) / TGT. **Зависит:** 3.5, 0.6. **Файлы шага:** —.
- **Что делать:** прогнать тест 3.5 на `XCL_ADAPTER=warp` (32-поточный); на TGT (если доступна) — на реальной 1080. grep на отсутствие wave-интринсиков уже в 3.3/3.4. Рандеву wave64→wave32 основного прохода не трогать.
- **Приёмка:** тест 3.5 зелёный на WARP (и на TGT, если есть; иначе «ждёт TGT» — не блокирует 3.7 по DEV, но блокирует G4).

#### 3.7 GATE G1: golden на retail-шейдерах тайтла
- **Машина:** DEV. **Зависит:** 3.6, 1.4. **Файлы шага:** `crates/xcl/tests/gpu_legacy_e2e.rs`.
- **Что делать:** расширить тест 3.5 на 3–5 реальных NGG-программ из дампа (e2e, образец — `crates/xcl/tests/gpu_pso_e2e.rs`; нужен `XCL_SAMPLE_RPF`). Нет дампа — `skip_test("no sample rpf")`, G1 помечается «условный», проверка переносится на TGT/7.2.
- **Приёмка:** ≥ 3 программ, 0 расхождений IB/args с референсом; список программ (индексы/имена) в журнале.

### Этап 4 — Graphics PSO и runtime draw-цепочка (indirect с самого начала)

#### 4.1 Эмиттер passthrough VS (vs_6_6)
- **Машина:** ANY. **Зависит:** 3.4. **Файлы шага:** `emit.rs` (новая функция), `legacy/reconstruction.rs`.
- **Что делать:** `passthrough_vs(record, ps_inputs) -> String`: по `SV_VertexID` читает глобальный id из IB (root SRV `t*`), затем (D3-A) вычисляет адрес вершины в `res` (root SRV): `rec = id / max_vertices`, `local = id % max_vertices` → pos + outputs; выдаёт `SV_Position` + интерполяторы строго по входу PS (образец — согласование входов в `mesh_passthrough_for`); clip/cull сохранить (1.3); flat-атрибуты по provoking vertex (D7); при per-primitive outputs — читать через `prim_src`.
- **Приёмка:** (1) компиляция в `vs_6_6`; (2) автоматический тест соответствия сигнатуры выхода VS входу PS (`ps_inputs`).

#### 4.2 Legacy root signature и graphics PSO
- **Машина:** DEV. **Зависит:** 4.1, 3.2. **Файлы шага:** `pso.rs`, `real.rs`. (≤400 строк; иначе 4.2a RS / 4.2b PSO.)
- **Что делать:**
  1. `legacy_root_signature_with()` (или расширение `abi_root_signature_with()` ~1046): те же 6 root UAV + root SRV для IB и `res` (+ `prim_src`/VB при необходимости) + root constants (если нужны `max_vertices` и пр.; предпочтительно — константы в HLSL, генерируемые под программу); порядок слотов зафиксировать в contract.md; heap-directly-indexed флаги и static samplers PS — как в mesh-PSO.
  2. `legacy_graphics_pso()` в `real.rs` (образец — `translated_graphics_pso()` ~4595): VS = `passthrough_vs`, PS = переведённый консольный PS без изменений; реестр `LEGACY_PSO` (по образцу `TWO_PASS` ~5117); счётчик `PSO_LEGACY`.
- **Приёмка:** юнит-тест (образец `passthrough_mesh_stream_accepts_a_triangle_driver_blob`, `real.rs` ~5225): на DEV создаётся legacy-PSO из синтетических шейдеров, device жив; `cargo test -p xcl --lib two_pass legacy` зелёные.

#### 4.3 CommandSignature и args
- **Машина:** DEV. **Зависит:** 4.2, 3.3. **Файлы шага:** `real.rs`, `legacy/indirect.rs`.
- **Что делать:** по образцу `dev_create_command_signature()` ~3237: signature типа `DRAW`, `ByteStride=16`; args-блок в регионе аллокатора (пишет scan-CS-финализация, D6); барьеры `UAV → INDIRECT_ARGUMENT` перед `ExecuteIndirect`, обратно после.
- **Приёмка:** юнит-тест создания signature (device жив); тест размера/выравнивания args-блока; golden 3.5 расширен проверкой args.

#### 4.4 Runtime draw-цепочка
- **Машина:** DEV. **Зависит:** 4.3. **Файлы шага:** `bind.rs`, `real.rs`, `frame.rs` (только вызов `StoreRing::begin_frame`).
- **Что делать:**
  1. `bind.rs::graphics_bound()` ~495 резолвит `LEGACY_PSO`.
  2. В draw-хуке при legacy-PSO: выделить регион из `StoreRing` (на `Overflow` — скип draw, `LEGACY_OVERFLOW`, warn) → `bind_abi()` (+ root SRV) → основной compute-проход уже выполнен тайтлом → scan-CS Dispatch → UAV barrier → recon-CS Dispatch → barrier + переходы состояний (`res`: UAV→SRV; IB; args→INDIRECT_ARGUMENT) → `SetPipelineState(legacy)` → `ExecuteIndirect(signature, args, 1, 0)`.
  3. Счётчики: `DRAWS_LEGACY`, `SCAN_DISPATCHES`, `RECON_DISPATCHES`, `INDIRECT_DRAWS`; лог пиков ёмкости за кадр (для проверки допущений 1.5).
- **Приёмка:**
  1. `cargo test -p xcl --lib bind` зелёные (существующие не сломаны).
  2. DEV, testtitle `legacy_backend="on"`: `DRAWS_LEGACY > 0`, `INDIRECT_DRAWS > 0`, `XCL_DRED=1` чисто, fence-таймаутов нет.
  3. `legacy_backend="off"` — smoke идентичен baseline 2.4.
- **Записать:** счётчики из `report.json`, пики ёмкости vs оценка 1.5.

#### 4.5 Инструмент сравнения кадров (для G2/G3)
- **Машина:** DEV. **Зависит:** 4.4. **Файлы шага:** `tools/imgdiff/*` (standalone), `frame.rs` (минимальный хук).
- **Что делать:**
  1. Хук `XCL_FRAME_SAVE=N`: на кадре N читать back render target (основной цветовой) и сохранять как raw/PNG во внешний каталог артефактов (`XCL_ARTIFACT_DIR` или путь из конфига); логировать путь+хэш.
  2. `tools/imgdiff`: сравнивает два изображения → `{identical: bool, max_abs_diff, mean_abs_diff, psnr_db, differing_pixels}` в JSON + diff-картинка.
  3. Для синтетического сравнения — тот же вывод получить из `runner`-бенча (RT читается в тесте).
- **Приёмка:** `imgdiff a.png a.png` → `identical=true`; `imgdiff a.png b.png` (разные) → корректные метрики (тест на фикстурах); `XCL_FRAME_SAVE=30` на DEV создаёт файл, путь+хэш в журнале.

#### 4.6 GATE G2: картинка legacy == mesh-эталон
- **Машина:** DEV. **Зависит:** 4.5. **Файлы шага:** новые тесты.
- **Что делать:**
  1. **Эталонное сравнение (авто):** т.к. runtime mesh-пути нет, эталон — `runner::mesh_draw_split` на тех же синтетических и 3–5 retail-программах (3.7): рендерить в RT mesh-способом и legacy-способом (4.4-цепочка в runner), сравнить RT через логику imgdiff. Допуск записать **до** прогона; ожидание — побитовое равенство для непрозрачной геометрии (`identical=true`), минимум PSNR ≥ 60 dB.
  2. **Тайтл (визуально):** прогон тайтла с `legacy_backend="on"`, `XCL_FRAME_SAVE` на нескольких кадрах (≥ 3 сцены) + `XCL_FRAME_DUMP` (статистика как вспомогательная метрика).
  3. Отчёт владельцу: метрики + скриншоты (пути/хэши). **Требуется явное подтверждение владельца** «визуально ок» → запись в «Подтверждения владельца».
- **Приёмка:** авто-сравнение в допуске (JSON в артефактах, хэши в журнале); подтверждение владельца записано. **Без обоих — этапы 5+ не начинать**; при провале — багфикс 4.x (или reopen 3.x).
- **Записать:** допуски, метрики, вердикт G2.

### Этап 5 — GPU culling (opt-in, D5)

#### 5.1 Cull-CS: frustum + стабильная компакция
- **Машина:** ANY (код) / DEV (тест). **Зависит:** 4.6. **Файлы шага:** `emit.rs` (новая функция), `legacy/cull.rs`.
- **Что делать:** cull-CS читает IB + `res`/VB, тестирует примитивы по D5 (все 3 вершины вне одной плоскости; `w<=0` — осторожно; без внешних параметров), выживших **стабильно** компактит в IB2 (scan по group-shared, без wave-интринсиков; порядок сохраняется) и пишет args (`VertexCountPerInstance = survived*3`); при `prim_src` — переупорядочивает его.
- **Приёмка:** golden на синтетической сцене: выжившие == CPU-референсу, содержимое и порядок IB2 совпадают; случаи: все видимы, все отсечены, примитив пересекает near-плоскость (не отсекается), примитив с `w<=0` вершиной (не отсекается, если не за одной плоскостью), >1024 примитивов (межблочная компакция), args == референс.

#### 5.2 Backface culling (флаг)
- **Машина:** DEV. **Зависит:** 5.1. **Файлы шага:** `legacy/cull.rs`, `emit.rs`.
- **Что делать:** флаг `cull_backface` (константа, подставляемая в HLSL при генерации); знак площади в clip-space с учётом обмотки/`FrontCounterClockwise` PSO. Если аудит 1.3 показал, что консольные NGG уже отсекают — флаг default off, тест подтверждает отсутствие двойного отсечения.
- **Приёмка:** флаг off → результат 5.1 идентичен; on → число уменьшается ровно на число задних граней референса.

#### 5.3 Hi-Z occlusion (опционально)
- **Машина:** DEV. **Зависит:** 5.1. НЕ реализовывать без отдельного решения владельца; иначе `deferred`.
- **Приёмка:** статус в журнале.

#### 5.4 Интеграция cull в цепочку (`cull_enable`)
- **Машина:** DEV. **Зависит:** 5.1, 4.4. **Файлы шага:** `bind.rs`, `real.rs`.
- **Что делать:** при `cull_enable=true`: recon → cull(IB→IB2, args) → barrier → `ExecuteIndirect` по IB2; при `false`: цепочка 4.4 без cull (cull-CS не создаётся); счётчик `CULL_PASSES`; (debug, под `XCL_FRAME_DUMP`) readback args для `LEGACY_PRIMS_CULLED`. Регион IB2 — из `StoreRing`.
- **Приёмка:** bind-тесты зелёные; testtitle `cull_enable=true`: `CULL_PASSES > 0`, DRED чисто; `false`: `CULL_PASSES == 0`.

#### 5.5 GATE G3: cull on == cull off
- **Машина:** DEV. **Зависит:** 5.4, 4.5. **Файлы шага:** —.
- **Что делать:** сравнить кадры (`XCL_FRAME_SAVE`, imgdiff) при `cull_enable=false` и `true` на тех же сценах, что G2; плюс авто-сравнение в runner-бенче. Отсечение — консервативное, поэтому ожидается **побитовое равенство** кадров; любое расхождение — баг cull (по D5 «сомнительное не отсекать»). Записать число отсечённых примитивов.
- **Приёмка:** `identical=true` (или задокументированное расхождение с разбором причины, одобренное владельцем); число отсечённых примитивов в журнале.

### Этап 6 — Устойчивость и конфигурация

Шаги 6.1–6.5 после 5.5 независимы друг от друга, кроме 6.5 (после 6.1 и 5.4).

#### 6.1 Многокадровая устойчивость аллокатора
- **Машина:** DEV. **Зависит:** 5.5. **Файлы шага:** `legacy/vbib.rs`, `frame.rs`, `bind.rs`.
- **Что делать:** проверить кольцо N=3 под нагрузкой: регион кадра K не переиспользуется, пока GPU не завершил кадр K (fence-привязка), счётчики/args не копятся между кадрами (каждый draw пишет свой регион, scan перезаписывает его целиком; при необходимости — `ClearUnorderedAccessViewUint` для заголовков), политика overflow (D8).
- **Приёмка:** unit-тест кольца (запись/чтение/переполнение/fence); stress testtitle ≥ 500 кадров: `LEGACY_OVERFLOW == 0`, committed-память процесса в начале/конце без роста (числа в журнал); динамика счётчиков за 100 кадров не монотонна.

#### 6.2 Собственный graphics-indirect тайтла (если используется)
- **Машина:** DEV. **Зависит:** 5.5. **Файлы шага:** `bind.rs`.
- **Что делать:** если тайтл вызывает `execute_indirect()` ~253 с legacy-PSO (по логам/`XCL_PSO_TRACE`), переадресовать на цепочку; учесть `fresh_indirect_arguments` ~332. Если в тайтле такого нет — `deferred` с записью (доказательство: счётчик/лог).
- **Приёмка:** либо тест/прогон с переадресацией, либо `deferred` + доказательство.

#### 6.3 Аудит барьеров (debug layer)
- **Машина:** DEV. **Зависит:** 6.1. **Файлы шага:** `bind.rs`.
- **Что делать:** `d3d12_debug=true`, прогон testtitle `legacy=on` (cull on и off); устранить все предупреждения по legacy-ресурсам (состояния UAV/SRV/INDIRECT_ARGUMENT).
- **Приёмка:** хвост лога без предупреждений debug layer по legacy (цитата).

#### 6.4 Финальная проверка изоляции диффа
- **Машина:** ANY. **Зависит:** 6.3. **Файлы шага:** —.
- **Что делать:** `git diff main...HEAD --stat`: изменения только в разрешённых файлах (Ч.0.6 п.5). Любые отклонения — выписать и обосновать (либо откатить). (Проверка на каждом шаге уже выполнялась по Ч.0.2 SCOPE; здесь — итог.)
- **Приёмка:** diff-статистика и список файлов в журнале; отклонения обоснованы.

#### 6.5 Финальные флаги конфига
- **Машина:** ANY. **Зависит:** 6.1, 5.4. **Файлы шага:** `config.rs`.
- **Что делать:** проверить комплект: `legacy_backend`, `cull_enable`, `cull_backface`, `legacy_budget_mb`; обновить тесты парсинга. Значение `cull_enable` по умолчанию определяется на 7.4 (по замеру); до этого — `false`.
- **Приёмка:** тесты config зелёные; testtitle с `cull_enable=false`: `CULL_PASSES == 0`.

### Этап 7 — Целевая машина GTX 1080 (TGT) и производительность

#### 7.1 TGT: актуализация сборки/среды
- **Машина:** TGT. **Зависит:** 6.5, G0 ok. **Файлы шага:** —.
- **Что делать:** `git pull --rebase`; `cargo build --release --workspace`; перепроверить версии инструментов (DXC 1.8+, Agility, `xgameruntime.dll`); `xcl inspect` на тайтл.
- **Приёмка:** сборка зелёная (вывод в журнал); `xcl run` testtitle стартует, `report.json` — GPU = GTX 1080.

#### 7.2 TGT: прогон тайтла в legacy-режиме (G4a)
- **Машина:** TGT. **Зависит:** 7.1. **Файлы шага:** —.
- **Что делать:** `legacy_backend="auto"`; прогон; собрать `report.json`, `xcl.log`, `XCL_FRAME_SAVE` на ≥ 3 сценах (+ `XCL_FRAME_DUMP`). Если G1 был условным (нет дампа на DEV) — выполнить e2e 3.7 здесь. Если G0 был fail — шаг замещается планом fallback (решение владельца).
- **Приёмка:** `PSO_LEGACY > 0`, `INDIRECT_DRAWS > 0`, device-removal нет; артефакты сохранены (хэши); скриншоты показаны владельцу.
- **Записать:** счётчики, FPS (если есть).

#### 7.3 TGT: legacy и compute-тесты на реальном wave32 (G4b)
- **Машина:** TGT. **Зависит:** 7.2. **Файлы шага:** только **новые** legacy-тесты; правки skip-условий существующих тестов — только по списку из 0.6 и минимально.
- **Что делать:** `cargo test -p xcl --lib legacy mesh recomp` на TGT; legacy-тесты (3.5, 3.7, 5.1, 5.2, 4.x) обязаны **выполняться** (не skip) и быть зелёными — их skip-условия не требуют mesh tier. Для существующих compute-only тестов, которые скипались по «нужен mesh» (список 0.6), допустима точечная правка условия **только** если тест не использует mesh-стадию; семантику `XCL_REQUIRE_GPU` менять нельзя; каждая такая правка — запись в «Решения».
- **Приёмка:** passed/failed/skipped в журнале, список skipped с причинами; legacy-тесты не skipped.

#### 7.4 TGT: замер производительности
- **Машина:** TGT. **Зависит:** 7.2. **Файлы шага:** —.
- **Что делать:** фиксированная сцена и длительность (описать в журнале); FPS/кадровое время (встроенный счётчик игры, если есть); конфиги: `cull_enable=false/true`. **Решение по `cull_enable` default:** включить по умолчанию только если выигрыш ≥ согласованного порога (записать порог до замера; иначе остаётся `false`).
- **Приёмка:** таблица FPS (≥ 3 замера × 2 конфига) в журнале; решение по default записано.

#### 7.5 Оптимизации
- **Машина:** TGT/DEV. **Зависит:** 7.4. **Файлы шага:** по месту.
- **Что делать:** по узким местам 7.4: размер группы recon (64/128/256), u16 IB при `max_vertices*records < 65536`… (только если применимо к глобальным id; иначе пропустить), батчинг recon/scan на несколько draw, переоценка D3-A vs D3-B по порогам 1.5 (если доля заполнения записей/кэш-промахи — узкое место). Каждая оптимизация — отдельный коммит + замер до/после + повтор G2/G3 (кадр не изменился).
- **Приёмка:** для каждой внедрённой — замер до/после; без улучшения — откат и запись.

#### 7.6 Регрессия на DEV
- **Машина:** DEV. **Зависит:** 7.5 (или 7.4). **Файлы шага:** —.
- **Приёмка:** `legacy_backend="off"` → smoke == baseline 0.4; `on` → G2/G3-сравнения не деградировали; свежие smoke-блоки в журнале.

### Этап 8 — Финализация

#### 8.1 Документация `docs/legacy-gpu-backend.md`
- **Машина:** ANY. **Зависит:** 7.6. **Файлы шага:** `docs/legacy-gpu-backend.md`.
- **Что делать:** что такое legacy-бэкенд; как включить (config); требования к GPU (SM 6.6 обязательно, mesh tier не нужен); последствия Pascal; результаты 7.4/7.5 (таблица); ограничения; ссылка на contract.md.
- **Приёмка:** документ в репо; цифры совпадают с журналом.

#### 8.2 Счётчики в report.json
- **Машина:** DEV/TGT. **Зависит:** 7.6. **Файлы шага:** `report.rs`, `real.rs` (`D3d12Stats`).
- **Что делать:** `legacy_pipelines`, `legacy_draws`, `legacy_scan_dispatches`, `legacy_recon_dispatches`, `legacy_cull_passes`, `legacy_indirect_draws`, `legacy_overflow`, `legacy_peak_store_mb`, `legacy_prims_culled` (последнее — только при `XCL_FRAME_DUMP`).
- **Приёмка:** `report.json` после прогона содержит все поля (jq/python-парсинг; значения в журнал) на обеих машинах.

#### 8.3 README форка
- **Машина:** ANY. **Зависит:** 8.1. **Файлы шага:** `README.md`.
- **Что делать:** секция «Legacy GPU backend (no mesh shaders)»: требования, включение, статус; ссылка на docs. Остальной README не переписывать.
- **Приёмка:** diff README — только добавленная секция.

#### 8.4 Закрытие проекта
- **Машина:** ANY. **Зависит:** 8.1–8.3. **Файлы шага:** `PROGRESS.md`.
- **Что делать:** финальный `.\scripts\test.ps1` + smoke на DEV и TGT; итоговая сводка (что сделано, ворота G0–G4 и подтверждения владельца, цифры 7.4/7.5); все шаги `done`/`deferred`. `HANDOFF.md` не трогать.
- **Приёмка:** все шаги закрыты; итоговые тесты зелёные (числа в журнале).

---

## Ч.6. Контрольные ворота и стоп-условия

| Ворота | Шаг | Условие | При провале |
|---|---|---|---|
| **G0** | 0.3 | SM 6.6 PSO создаётся на TGT | СТОП всего плана; решение владельца |
| **G1** | 3.7 | recon-CS (scan+recon) == референс/mesh-проход на retail-шейдерах, побайтно, с порядком | багфикс 3.x; этапы 4+ не начинать |
| **G2** | 4.6 | legacy-кадр == mesh-эталон (побитово или PSNR ≥ допуска) **и** визуальное подтверждение владельца | багфикс 4.x; этапы 5+ не начинать |
| **G3** | 5.5 | cull on == cull off (побитово) | багфикс 5.x; `cull_enable` остаётся false |
| **G4** | 7.2/7.3 | тайтл идёт на 1080; legacy-тесты на wave32 зелёные и не skipped; визуальное подтверждение владельца | блокер → владелец |

Ворота — обязательные точки отчёта владельцу (короткое сообщение: ворота X = pass/fail, цифры, что дальше). Для G2 и G4 владелец подтверждает явно (запись в «Подтверждения владельца»).

## Ч.7. Известные ловушки (читать ДО соответствующих этапов)

1. **Pascal и SM 6.6** — XCL требует 6.6 везде (`cs_6_6/ms_6_6/ps_6_6` захардкожены; `no_sm66` гейтится в preflight). Решается пробой 0.3, не догмами.
2. **Runtime mesh-пути нет** — не ищи `DispatchMesh` в runtime: его нет (TODO HANDOFF ~1227). Эталон — `runner::mesh_draw_split`. Legacy реализует draw-цепочку **первым**; поэтому G2 опирается на runner-эталон и визуальную проверку владельца.
3. **32 КБ лимит mesh-выходов** — для legacy не критичен, но split-расчёт групп наследуется от `passthrough_split`, чтобы покрыть все записи.
4. **Порядок примитивов** детерминирован (D2: scan). Любое решение с атомарным append запрещено без записи в «Решения» — оно ломает blending/декали и воспроизводимость кадров.
5. **`skip_test`, не `#[ignore]`** — legacy-тесты на TGT не должны скипаться: их условия не требуют mesh tier.
6. **Сгенерированные файлы** — `*_spec.rs`, tiling-таблицы правятся только генераторами `scripts/`.
7. **`cargo fmt` запрещён**; форматируй только свои строки.
8. **≤ 400 строк на логическое изменение** — делить a/b/c.
9. **HANDOFF.md / docs/specs/** — не редактировать; расхождения — в PROGRESS.md.
10. **10-секундный fence-таймаут** в runner: расхождение рандеву wave64/wave32 проявляется детерминированным фейлом теста — не «чинить» таймаутом.
11. **VRAM 11 ГБ у 1080**: `res` разрежен (по `max_vertices/max_primitives` на группу) и уже занимает память compute-прохода; IB/scan/args — поверх. Следить за ёмкостью (D8), не аллоцировать «с запасом ×N».
12. **DXC 1.7** даёт 2–2.7× DXIL и медленные компиляции — на TGT ставить DXC 1.8+.
13. **Состояния ресурсов:** `res` — UAV в compute-проходе и SRV в VS; требуется переход `UAV→NON_PIXEL_SHADER_RESOURCE` и обратно; args — `UAV→INDIRECT_ARGUMENT`. Не полагаться на неявное продвижение состояний.
14. **Provoking vertex / flat / per-primitive** (D7): при неиндексированном fetch-`Draw` provoking зависит от порядка вершин в тройке; проверять на сценах с flat-интерполяторами.
15. **Деление на `max_vertices` в VS** — не степень двойки в общем случае; допустимо (целочисленное деление), но при профилировании 7.5 рассмотреть выравнивание stride вершин записи до степени двойки только в D3-B.
16. **Числа в журнал — только из файлов/команд, не из памяти.**

## Ч.8. Если заблокирован

1. Три безуспешные попытки на шаге → `blocked`: причина, гипотезы, минимальный репро-вывод.
2. Нужен владелец (нет TGT, нет дампа, G0 fail, подтверждение G2/G4, решение по deferred) → `blocked` + отчёт.
3. Заблокированный шаг не блокирует независимые ветки (напр. 6.1–6.4 между собой; 8.1/8.2/8.3).
4. Всё заблокировано → финальный отчёт владельцу: что сделано (коммиты), что блокирует, что нужно.

## Приложение A. Bootstrap-промпт (вставляет владелец новому агенту)

```text
Работай в корне репозитория XCL (форк xlc-legacy-gpu-backend). Твоя машина: <DEV | TGT>.
Главные файлы: AGENT_PLAYBOOK.md (план и правила — выполняй буквально) и PROGRESS.md (текущий статус работы).
Начни с bootstrap-цикла из раздела Ч.0 плейбука: проверь ветку (legacy-gpu-backend), подтяни изменения (git pull --rebase, если есть remote),
прочитай PROGRESS.md, спот-чеком прогони приёмку двух последних done-шагов ТВОЕЙ машины (или ANY), затем продолжай с первого доступного шага не-done для твоей машины.
Каждый шаг: сделать → проверить SCOPE диффа → выполнить всю приёмку → обновить PROGRESS.md → коммит по формату Ч.0.5.
Не помечай шаг done без реального вывода приёмки в журнале. На воротах G2/G4 жди явного подтверждения владельца.
При блокировке — статус blocked с причиной и стоп с отчётом.
Если подходит конец контекста/лимита — выполни чек-лист передачи (Ч.0.4) и остановись.
```

## Приложение B. Короткая справка команд

```powershell
# сборка и тесты
cargo build --release --workspace
.\scripts\test.ps1
cargo test -p xcl --lib <filter>

# запуск
xcl run D:\Game
xcl inspect D:\Game\game.exe
xcl shaders D:\data\shaders.rpf --precompile --only cs --pipelines

# регрессия
.\scripts\smoke.ps1 -Game <folder>

# диагностика
$env:XCL_DRED="1"; $env:XCL_PSO_TRACE="1"; $env:XCL_FRAME_DUMP="30"; $env:XCL_FRAME_SAVE="30"
$env:XCL_REQUIRE_GPU="wave64"    # в CI: скип = фейл
$env:XCL_ADAPTER="warp"          # 32-поточный софт-адаптер для проверок wave32

# журнал/передача
git branch --show-current; git log --oneline -5
git status --short
git diff --stat
git pull --rebase
```
