# Avidentifierare

Ett skrivbordsprogram som maskar känsliga personuppgifter i svensk text — **tryggt nog att klistra in i en kraftfull AI** som Claude. Allt körs **lokalt** på datorn; ingenting skickas till internet.

Byggt för icke-teknisk personal inom t.ex. **skola, elevhälsa och socialtjänst**. En fil att installera, dubbelklicka och kör.

## Funktioner

- **Flera detektionslager** som kombineras och granskas av människan:
  - **KB-BERT NER** (KBLab) — namn, platser, organisationer, tider
  - **Regler** — personnummer (Luhn-validerade), telefon, e-post, IP, ICD-10-koder
  - **Inbyggda ordlistor** — svenska diagnoser (togglas av/på)
  - **Egen ordlista** — egna termer, enskilt eller i bulk
  - **Valfritt AI-lager** — en liten lokal språkmodell (Qwen2.5-1.5B via candle) som fångar kontextuella/indirekta ledtrådar
- **Profiler** (Allmän, Skola/Elevhälsa, Socialtjänst) som förväljer kategorier
- **Granskning** — varje träff visas markerad; godkänn/avvisa innan export
- **Konsekvent pseudonymisering** — samma person blir "Person 1" genom hela dokumentet
- **In/ut**: vanlig text och Word (.docx). Kopiera till urklipp, eller spara som .txt/.docx

> Ingen automatik fångar 100 %. Granskningssteget är själva säkerheten — granska alltid innan du delar.

## Teknik

Tauri 2 (Rust-backend) + SvelteKit (gränssnitt). NER körs via ONNX Runtime (`ort`), LLM via `candle` — båda inbyggda i binären, inga externa runtimes. Modellerna bäddas in i den färdiga installern.

## Bygga från källkod

### Förutsättningar
- **Rust** (stable-msvc) + Visual Studio Build Tools (C++ + Windows SDK)
- **Node.js** 18+
- **Python 3.x** (behövs bara för att bygga ONNX-NER-modellen)

### Steg
```powershell
npm install

# Modellerna checkas inte in i git — bygg/hämta dem en gång:
model-tools\build-model.ps1   # konverterar KB-BERT -> src-tauri/resources/model (kräver Python)
model-tools\fetch-llm.ps1     # hämtar Qwen2.5-1.5B GGUF -> src-tauri/resources/llm

# Utveckling (öppnar appen):
npm run tauri dev

# Bygg installer (NSIS setup.exe):
npm run tauri build
# -> src-tauri/target/release/bundle/nsis/Avidentifierare_x.y.z_x64-setup.exe
```

## Projektstruktur
```
src/                     SvelteKit-gränssnitt (granskningsvyn)
src-tauri/src/
  pii/                   detektorer: model (NER), rules, gazetteer, dictionary, pseudonym, merge
  ai.rs                  valfritt LLM-lager (candle)
  docio.rs               txt/docx läs & skriv
  engine.rs              orkestrering: detektera -> granska -> exportera
model-tools/             engångsskript för att bygga/hämta modeller
mockups/                 designmockuppar (HTML)
samples/                 exempeltext
```

## Modeller & licenser
- **NER:** [KBLab/bert-base-swedish-cased-ner](https://huggingface.co/KBLab/bert-base-swedish-cased-ner) (konverteras till int8-ONNX lokalt)
- **LLM (valfritt lager):** [Qwen2.5-1.5B-Instruct](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct) — Apache-2.0 (GGUF Q4_K_M via bartowski)

Se respektive modells sida på Hugging Face för licensvillkor.

## Distribution
Den färdiga installern är ~1 GB (mestadels AI-modellen) och läggs lämpligen upp som en **GitHub Release**-bilaga, inte i repot. Installern är osignerad — Windows SmartScreen visar en "okänd utgivare"-varning tills ett kodsigneringscertifikat används.
