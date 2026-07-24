# full_adder_demo

Veryl 純正シミュレータ（native backend）で 1 ビット全加算器をシミュレーションし、
波形を **VCD / FST** で出力して GTKWave で表示できることを示すデモプロジェクト。

## 構成

- `src/full_adder.veryl` — 組み合わせ回路の全加算器 `FullAdder` と、
  クロックで 8 入力パターン（000〜111）を印加するテストベンチ `test_full_adder`。
  組み合わせ回路だが、波形はクロックエッジごとにダンプされるため
  `$tb::clock_gen` を使って 1 サイクルずつ入力を与えている。

## 実行

```bash
veryl test --wave
```

- `1 passed, 0 failed` になれば成功。
- 波形は `Veryl.toml` の `[test]` 設定に従って `wave/` 以下に出力される。

## 波形フォーマットの切り替え（Veryl.toml）

```toml
[test]
waveform_format = "fst"   # "vcd" or "fst"

[test.waveform_target]
type = "directory"
path = "wave"
```

- `vcd` … テキスト形式。汎用性が高くどのツールでも読める。
- `fst` … GTKWave ネイティブのバイナリ形式。圧縮＋部分ロードでファイルが小さく高速。

## GTKWave で開く

```bash
gtkwave wave/test_full_adder.fst   # または wave/test_full_adder.vcd
```

## おまけ

`waveform.html` … GTKWave が無い環境向けに、ダンプされた波形（タイミング図）と
真理値表をブラウザで確認できる静的ビューア。
