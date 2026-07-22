# N-FDL v1 — язык описания сетевых форматов

**N-FDL** (Network Format Description Language) — декларативный язык описания бинарных сетевых протоколов и среда выполнения (виртуальная машина байткода + расширенный конечный автомат, EFSM), которая разбирает трафик, проверяет ограничения и управляет автоматами сессий.

## Возможности (текущая версия)

- **Парсер**: поля, длины, зависящие от выражений (`bytes[length-2]`), локальные определения `let`, `__current_offset`, циклы `loop ... while (expr)`, вложенные сообщения (`MessageRef`), автоматы `state_machine { ... on Msg guard (...) -> State { set ...; emit ...; } }`.
- **Виртуальная машина байткода**: генерация и выполнение компактного байткода; циклы через переходы, арифметические и логические выражения, переменные длины полей.
- **Исполнитель (runner)**: сквозной разбор протокола; контекст из полей, `let` и циклов; интеграция с EFSM.
- **EFSM**: вычисление охранных условий переходов и действий (`set` — переменные потока, `emit` — события); смена состояний (например, IDLE → PENDING при RADIUS `code == 1`).
- **Ограничения ресурсов**: лимиты числа инструкций, глубины рекурсии `MessageRef`, размера контекста.
- **Примеры**: полная поддержка `radius.nfdl` (циклы, ссылки на `Attribute`, автомат состояний) и других протоколов из `docs/examples/`.

Контрольный список релизных гейтов — в [`PRODUCTION_CHECKLIST.md`](PRODUCTION_CHECKLIST.md). План развития — [`docs/spec/13-roadmap.md`](docs/spec/13-roadmap.md). Tooling: [`docs/tooling/lints.md`](docs/tooling/lints.md), [`docs/tooling/includes.md`](docs/tooling/includes.md).

## Быстрый старт

```bash
cargo build -p nfdl-cli

cargo run -p nfdl-cli -- radius.nfdl

cargo test -p nfdl-syntax
cargo test -p nfdl-runtime --test fsm_integration -- --nocapture
```

## Пример: автомат RADIUS (фрагмент `radius.nfdl`)

```nfdl
let attrs_len = length - 20;
let start_offset = __current_offset;

loop attrs
    while (__current_offset - start_offset) < attrs_len
{
    attr: Attribute;
}

state_machine AuthDialog {
    key = (bidir(UDP.src_port, UDP.dst_port), identifier);

    state IDLE {
        on AccessMessage guard (code == 1) -> PENDING {
            set req_auth = authenticator;
        };
    }
    state PENDING { ... }
}
```

Движок:

1. выполняет цикл и разворачивает `MessageRef` в байткод;
2. собирает контекст (`code`, `authenticator`, `attrs_len` и т.д.);
3. проверяет охранное условие → выполняет переход и `set`;
4. возвращает итоговое состояние и события.

## Архитектура

- **Лексер и парсер** (`nfdl-syntax`): рекурсивный спуск, актуальный синтаксис N-FDL.
- **Байткод** (`nfdl-runtime`): `Instruction` + `BytecodeVm` (выполнение по указателю команд, без интерпретации на критическом участке).
- **Исполнитель и EFSM**: `parse_and_run_with_data` → VM → контекст → `FsmEngine::feed`.
- **Лимиты и ошибки**: `RuntimeError::LimitExceeded`, ограничение глубины (доработка в v1.5).
- В ядре — без `unsafe`.

## Статус v1

Рабочий конвейер: парсер → байткод → VM → контекст → действия EFSM; покрыт тестами на реальных протоколах.

## План развития

- **v1** (текущая): парсер, VM, EFSM, базовые лимиты, автоматы состояний.
- **v1.5**: типизированная модель ошибок, усиление защиты, покрытие тестами, несколько типов сообщений.
- **v2**: сборка потоковых сегментов, фаззинг, формальные методы, производительность.

## Сборка и тесты

- Рабочее пространство Cargo: `nfdl-syntax`, `nfdl-runtime`, `nfdl-verify`, `nfdl-fuzz`, `nfdl-cli`.
- Сквозной тест: `fsm_integration`.
- При необходимости: `export PATH="$HOME/.cargo/bin:$PATH"`.

Актуальные релизные гейты и ссылки на tooling — в [`PRODUCTION_CHECKLIST.md`](PRODUCTION_CHECKLIST.md).

**Готово к исследовательскому использованию и поэтапной подготовке к промышленной эксплуатации.**
