# ywatchy

[English](README.md)

プロジェクトのフォルダを監視し、Claude Codeのskillのリンクを作成して、`CLAUDE.md`を同期するWindowsのコマンドラインツール。

- 各プロジェクトにあるskillのフォルダへのリンクを`~/.claude/skills/`に作成し、どのプロジェクトでもskillを使えるようにする
- 各プロジェクトの`CLAUDE.md`を保存先のフォルダ(store)にコピーし、元の`CLAUDE.md`とコピーを双方向に同期する

## インストール

[Scoop](https://scoop.sh/)でインストールする。

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install yscoopy/ywatchy
```

または、[Releases](https://github.com/ycookiey/ywatchy/releases)から`ywatchy.exe`をダウンロードする。

## 使い方

1. `ywatchy`を実行する。初回の実行では、`%APPDATA%\ywatchy\ywatchy.toml`を作成して終了する
2. 作成された設定ファイルで、`[sync]`と`[skills]`の両方の`scan_dirs`に、プロジェクトを置いたフォルダを指定する。例: `scan_dirs = ["C:/path/to/projects"]`
3. `ywatchy`をもう一度実行する。1回同期して結果を表示し、Ctrl+Cを押すまで監視を続ける。サインイン時に自動では起動しない

`scan_dirs`に指定したフォルダの直下の各フォルダを、1つのプロジェクトとみなす。

- skill: `<project>/skills/<name>/`と`<project>/.claude/skills/<name>/`のうち`SKILL.md`を含むフォルダを、`~/.claude/skills/<name>`としてリンクする。リンクにはディレクトリのシンボリックリンクを使い、Windowsがシンボリックリンクの作成を許可しないときはジャンクションを使う。`SKILL.md`が削除されると、ywatchyがリンクを削除する。起動時には、`~/.claude/skills`のリンクのうち、`scan_dirs`のフォルダの中を指し、見つかったどのskillのリンクでもないリンクを削除する
- CLAUDE.md: `<project>/CLAUDE.md`を、無ければ`<project>/.claude/CLAUDE.md`を、`<store>/<project>/CLAUDE.md`にコピーする。元の`CLAUDE.md`とコピーのどちらを編集しても、もう一方を同じ内容に更新する。起動時に2つのコピーの内容が異なるときは、更新日時が新しいほうで古いほうを上書きする

オプション: `--config <path>`で別の設定ファイルを使う。`-V`・`--version`でバージョンを、`-h`・`--help`で使い方を表示する。

## 設定

| キー | 初期値 | 意味 |
|---|---|---|
| `general.log_dir` | `"logs"` | ログのフォルダ。相対パスは設定ファイルのフォルダを起点にする |
| `general.log_level` | `"info"` | `trace`・`debug`・`info`・`warn`・`error`のいずれか。環境変数`RUST_LOG`があれば、`RUST_LOG`の値を優先する |
| `sync.scan_dirs` | `[]` | `CLAUDE.md`を同期するプロジェクトを置いたフォルダ |
| `sync.claude_md_store_dir` | `""` | 保存先のフォルダ。空なら設定ファイルのフォルダ |
| `sync.exclude_projects` | `["ywatchy"]` | 同期しないプロジェクトのフォルダ名。大文字と小文字を区別しない |
| `skills.scan_dirs` | `[]` | skillを探すプロジェクトを置いたフォルダ |
| `skills.target_dir` | `""` | リンクを作成するフォルダ。空なら`~/.claude/skills` |
| `skills.skill_patterns` | 上の2つのパス | プロジェクトからの相対パス。skillの名前の位置に`*`を1つ置く |
| `skills.exclude_projects` | `[]` | skillをリンクしないプロジェクトのフォルダ名。大文字と小文字を区別しない。作成済みのリンクは次の起動時に削除する |
| `watcher.debounce_ms` | `500` | ファイルの変更の通知をまとめる待ち時間。単位はミリ秒 |

## 既知の制限

- 動作を確認したのはWindowsだけ
- 起動時の同期では、新しいほうの`CLAUDE.md`で古いほうを上書きする。バックアップは作成しない
- 実行中に新しいskillをリンクするのは、ywatchyの起動時にあった`skills/`か`.claude/skills/`のフォルダの中だけ。ほかのフォルダのskillと、`skills.skill_patterns`に追加したパターンにだけ一致するskillは、次の起動時にリンクする
- 既存のプロジェクトのフォルダに追加した`CLAUDE.md`は、次の起動時から同期する
- skillのフォルダごと削除すると、`~/.claude/skills`に行き先の無いリンクが残る。残ったリンクは手で削除する
- 同じ名前のskillが2つ以上あるときは、最初に見つけた1つだけをリンクする

## アンインストール

1. Ctrl+Cでywatchyを止める
2. `scoop uninstall ywatchy`を実行するか、`ywatchy.exe`を削除する
3. `~/.claude/skills`のリンクと`%APPDATA%\ywatchy`のフォルダは残る。`%APPDATA%\ywatchy`には、設定ファイルとログが入っている。`sync.claude_md_store_dir`が空のときは、各プロジェクトの`CLAUDE.md`のコピーも入っている。不要なら削除する

## ビルド

Rustのツールチェーンが必要。実行ファイルの出力先は`target\release\ywatchy.exe`。

```powershell
cargo build --release
```

## ライセンス

[MIT](LICENSE)
