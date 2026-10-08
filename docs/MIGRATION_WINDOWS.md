# Переезд engine13: Steam Deck (Linux) → ПК Windows

Составлено 2026-10-08 по фактическому состоянию Deck. Каждый пункт «что где лежит» проверен
по коду или по диску; где не проверен — сказано прямо.

Короткая версия: код живёт на GitHub, его достаточно склонировать. Переносить руками надо
четыре вещи — **ключ LLM, базу сейвов, память Claude и (на всякий случай) git-бандл**.
`target/` (30 ГБ) не переносить.

---

## 1. Инвентарь: что где лежит на Deck

| что | где на Deck | есть на GitHub? | переносить |
|---|---|---|---|
| код, история `main`, все смёрженные PR | `~/Downloads/engine13`, `origin = git@github.com:qrewq5003-oss/engine13.git` | да | клон (§4) |
| 2 stash'а: `WIP run_id + lint (task23 setup)`, `WIP on fix/regression-scroll-and-event-reset` | локальный git | **нет** | только через бандл (§2.1) |
| `investigate/vassalage-submission-metric`: 1 коммит `04333f3`, которого нет в `main` (ещё 3 «впереди upstream» уже в `main` под теми же патчами) | локально | нет | через бандл |
| `investigate/tag-channel`: 1 коммит впереди upstream — его патч уже в `main` (`git cherry` = `-`) | локально | не нужен | — |
| ~14 старых локальных веток фаз 1–8 без upstream | локально | нет | через бандл, если хочется; рабочей ценности нет |
| старые worktree в `~/Downloads/engine13-worktrees/` (task20–27, ref-main-A) | диск | — | **не переносить**: исследования закрыты, выводы в `docs/` |
| ключ и настройки LLM | `~/.config/engine13/config.json` (поля `provider`, `api_key`, `model`, `base_url`) | **нет, и не должен** | вручную, не через git (§2.2) |
| сейвы игры (SQLite) | `~/.local/share/engine13/engine13.db` | нет | файлом (§2.3) |
| память Claude Code (104 файла, ~0,7 МБ) | `~/.claude/projects/-home-deck-Downloads-engine13/memory/` | нет | архивом (§2.4) |
| транскрипты прошлых сессий Claude (`*.jsonl` рядом с `memory/`) | там же | нет | по желанию; для работы не нужны |
| эталоны побайтовой проверки (`b43_sim.txt`, `s1_drift.txt`) | были в scratchpad сессий в `/tmp` | нет | **уже утеряны** — пересоздать на Windows (§6) |
| `target/` (30 ГБ), `src-tauri/target/` (1,4 ГБ), `node_modules/` | диск | — | **не переносить**, пересобираются |

Откуда пути: база — `Db::default_path()` = `dirs::data_dir()/engine13/engine13.db`
(`src/db.rs`); конфиг LLM — `dirs::home_dir()/.config/engine13/config.json`
(`src/llm/mod.rs::get_llm_config`).

---

## 2. Что сделать на Deck перед отъездом

### 2.1. Git-бандл — страховка для всего локального

Бандл — один файл со всеми ветками, тегами и коммитами. Stash в `--all` **не входит**, поэтому
сначала превратить stash'и в ветки:

```bash
cd ~/Downloads/engine13
git branch stash/run-id-lint      stash@{0}
git branch stash/scroll-event-reset stash@{1}
git bundle create ~/engine13-all.bundle --all
git bundle verify ~/engine13-all.bundle     # должен сказать "is okay"
```

`git branch <имя> stash@{N}` создаёт ветку на коммите stash'а; сам stash при этом не
удаляется.

### 2.2. Ключ LLM

`~/.config/engine13/config.json` содержит API-ключ открытым текстом. Не класть в репозиторий,
не пересылать в мессенджеры и почту. Варианты: флешка; или просто не переносить файл, а ввести
ключ заново в настройках приложения на Windows (приложение само создаст файл через
`save_llm_config`).

### 2.3. Сейвы

Закрыть приложение (чтобы SQLite не писал в файл во время копирования), затем скопировать
`~/.local/share/engine13/engine13.db`. Если рядом лежат `engine13.db-wal` / `-shm` — копировать
их вместе с базой.

### 2.4. Память Claude

```bash
tar czf ~/engine13-claude-memory.tgz -C ~/.claude/projects/-home-deck-Downloads-engine13 memory
```

### 2.5. Унести файлы

На флешку или в облако: `engine13-all.bundle`, `engine13.db` (+ `-wal/-shm`, если были),
`engine13-claude-memory.tgz`, при желании `config.json`.

---

## 3. Установить на Windows

Порядок важен: Build Tools до Rust.

1. **Visual Studio Build Tools 2022** → нагрузка «Разработка классических приложений на C++»
   (MSVC + Windows SDK). Это замена distrobox-контейнера `dev`: на Deck нет `cc`, на Windows
   линкер даёт MSVC.
2. **WebView2** — в Windows 10/11 обычно уже стоит; если нет — Evergreen Runtime с сайта
   Microsoft. Нужен Tauri.
3. **Rust** через `rustup-init.exe`, тулчейн по умолчанию `x86_64-pc-windows-msvc`. Версию
   ставить руками не надо: `rust-toolchain.toml` закрепляет `1.93.1`, rustup подтянет её при
   первом `cargo` в папке проекта. Если `cargo clippy` скажет, что компонента нет:
   `rustup component add clippy --toolchain 1.93.1`.
4. **Node.js 20.x, любой патч** (сейчас последний 20.20.2; на Deck стоит 20.20.0 — разница в
   третьей цифре только исправления, брать 20.20.2) — установщик с nodejs.org или nvm-windows.
   Почему 20, а не новее: CI собирает фронтенд на `node-version: 20`, и локально стоит держать
   то же, чтобы «у меня собралось» значило «соберётся в CI». Переход на 22/24 возможен, но
   делать его отдельной правкой — одновременно в `.github/workflows/ci.yml` и локально.
5. **Git for Windows** — и сразу, **до клона**:
   ```powershell
   git config --global core.autocrlf false
   git config --global user.name "lolo"
   git config --global user.email "<ваш email>"
   ```
   Почему `autocrlf false`: в репозитории нет `.gitattributes`, а установщик Git for Windows
   по умолчанию включает `autocrlf=true` и переводит файлы в CRLF. Сценарии встраиваются в
   бинарь через `include_str!` (`src/scenarios/*.rs`) — с CRLF в строки контента попадут `\r`,
   и мир перестанет совпадать побайтово с тем, что мерилось на Linux и в CI.
6. **GitHub CLI** (`gh`) и вход: `gh auth login`. Remote репозитория — SSH; ключ
   `~/.ssh/id_ed25519` с Deck либо перенести (приватный ключ — так же осторожно, как API-ключ),
   либо создать новый (`ssh-keygen -t ed25519`) и добавить его в GitHub (`gh ssh-key add`).
   Можно и проще: `gh auth login` с HTTPS и `git remote set-url origin https://github.com/qrewq5003-oss/engine13.git`.
7. **Claude Code** — установить и войти той же учётной записью.

---

## 4. Получить код

```powershell
cd C:\dev                      # любой короткий путь без кириллицы и пробелов
git clone git@github.com:qrewq5003-oss/engine13.git
cd engine13
git config core.autocrlf       # должно вывести false
```

Короткий путь — не каприз: у MSVC и у глубоких путей `target\` бывают проблемы с лимитом длины
пути в 260 символов.

Если нужны stash'и или неопубликованные ветки — подтянуть их из бандла:

```powershell
git fetch C:\путь\engine13-all.bundle "refs/heads/*:refs/remotes/bundle/*"
git branch -r | findstr bundle             # посмотреть, что пришло
git switch -c stash/run-id-lint bundle/stash/run-id-lint   # пример
```

---

## 5. Перенести данные

| что | куда на Windows |
|---|---|
| `engine13.db` (+ `-wal/-shm`) | `%APPDATA%\engine13\engine13.db` (т. е. `C:\Users\<имя>\AppData\Roaming\engine13\`) — так отвечает `dirs::data_dir()` на Windows; папку создать, если её нет |
| `config.json` | `C:\Users\<имя>\.config\engine13\config.json` — код берёт буквально `home_dir()\.config\engine13`, а не `%APPDATA%` |
| память Claude | см. ниже |

Путь базы можно не угадывать: приложение при старте печатает в консоль
`[RUST] Database path: ...`. Запустить один раз (§7), посмотреть путь, закрыть, положить файл
туда, запустить снова.

**Память Claude.** Claude Code хранит проект в `~/.claude/projects/<путь проекта, где
разделители заменены на «-»>/`. На Windows имя будет другим (для `C:\dev\engine13` —
что-то вроде `C--dev-engine13`). Надёжный способ: запустить `claude` один раз в папке проекта
на Windows, найти появившуюся папку в `C:\Users\<имя>\.claude\projects\` и распаковать в неё
`memory\` из архива. Проверка — в новой сессии Claude должен видеть индекс `MEMORY.md`.

После переноса поправить в памяти одну вещь, которая на Windows станет неверной:
`build-via-distrobox.md` («на хосте нет cc; `distrobox enter dev -- cargo test`»). На Windows
команды запускаются напрямую: `cargo test`, без distrobox. Проще всего попросить об этом Claude
в первой сессии.

---

## 6. Проверить, что всё собирается

Ровно то, что гоняет CI (`.github/workflows/ci.yml`); проверять **по коду возврата**, а не по
отсутствию слова FAILED в выводе:

```powershell
npm ci
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo clippy --workspace --features census -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npx tsc --noEmit
npm run build
```

Первая сборка будет долгой — `target\` собирается с нуля.

**Эталоны побайтовой проверки пересоздать на Windows, а не везти с Deck.** Старые утеряны, но и
целые они бы не годились: `f64::powf`, `ln`, `exp` и т. п. идут в математическую библиотеку
платформы (glibc на Linux, CRT от Microsoft на Windows), и последние знаки могут отличаться.
Сравнивать «до/после» правки можно только в пределах одной машины. Порядок:

```powershell
git switch main
cargo run --release --bin sim -- <аргументы как в прежних проверках>   > base\sim.txt
cargo run --release --bin drift_probe                                   > base\drift.txt
```

(аргументы — как в соответствующих задачных документах; `drift_probe` понимает `ECONOMY_V2=1`
— в PowerShell `$env:ECONOMY_V2=1`). CI на Linux при этом остаётся арбитром: то, что прошло
у вас и в CI, — прошло.

Бинари на Windows — `target\release\sim.exe` и т. п.; bash-однострочники из прошлых отчётов
запускать в Git Bash или переписывать на PowerShell.

---

## 7. Запустить приложение

```powershell
npm run tauri dev
```

В консоли должно появиться `[RUST] Database path: ...` с путём из §5. Проверить вручную:

- список сейвов виден, старый сейв загружается (значит, база перенесена в правильное место);
- хроника генерируется (значит, `config.json` найден и ключ рабочий);
- панель «Летопись» под хроникой (книга партии, PR #243): она ещё не проверялась в
  живом GUI, это хороший момент проверить.

---

## 8. После переезда

- Deck не стирать, пока на Windows не прошли §6 и §7.
- Старые worktree и `target/` на Deck можно удалить потом — на GitHub ничего из них не нужно.
- Если API-ключ ездил на флешке — удалить его оттуда.

## Чего этот документ не проверял

- Работу Tauri-сборки на Windows: CI собирает `src-tauri` только на Linux (`ubuntu`).
  Платформенных `#[cfg]` в коде проекта нет, но первым запуском на Windows это и проверяется.
- Точное имя папки проекта Claude на Windows — поэтому в §5 способ «запустить и посмотреть».
