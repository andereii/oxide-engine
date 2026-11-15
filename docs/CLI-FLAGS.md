# CLI Flags y Variables de Entorno

El ejecutable principal de **Oxide Engine** (`oxide`) acepta flags por línea de comandos para lanzar `PlayState` directamente con la canción, dificultad y opciones deseadas.

## Sintaxis soportada

Para flags con valor se aceptan los tres formatos:

- `--flag:valor` (recomendado, ej. `--song:blammed`)
- `--flag=valor` (ej. `--song=blammed`)
- `--flag valor` (ej. `--song blammed`)

Para flags booleanos basta con pasar el flag (equivale a `true`), o usar `:true` / `:false` (`=true` / `=false`).

## Flags disponibles

| Flag | Alias | Tipo | Por defecto | Descripción |
| :--- | :--- | :--- | :--- | :--- |
| `--song:<id>` | — | `string` | `blammed` | ID de la canción dentro de `assets/gameplay/songs/<id>/`. |
| `--difficulty:<diff>` | `--diff:<diff>` | `string` | `hard` | Dificultad a cargar (`easy`, `normal`, `hard`, `erect`, `nightmare`). |
| `--variation:<var>` | — | `string` | `default` | Variación del chart. |
| `--botplay` | `--bot-play` | `bool` | `false` | Activa el modo botplay en la strumline del jugador (si se pasa el flag, queda en `true`). |
| `--practice` | — | `bool` | `false` | Activa el modo práctica (evita game over por vida en `0`). |
| `--minimal` | — | `bool` | `false` | Activa el modo mínimo (sin stage ni personajes). |
| `--rate:<f64>` | `--playback-rate:<f64>` | `f64` | `1.0` | Multiplicador de velocidad de reproducción (`0.1` a `10.0`). |
| `--start-ms:<f64>` | — | `f64` | `0.0` | Posición inicial de la canción en milisegundos (`>= 0.0`). |
| `--vsync` / `--no-vsync` | — | `bool` | `OXIDE_VSYNC` (`true`) | Fuerza activar o desactivar VSync en la ventana. |
| `--help` | `-h` | — | — | Muestra la ayuda de flags en consola y sale sin abrir ventana. |

## Variables de entorno

| Variable | Por defecto | Descripción |
| :--- | :--- | :--- |
| `OXIDE_VSYNC` | `1` (`true`) | Si vale `0`, desactiva VSync al iniciar (sobrescribible con `--vsync` / `--no-vsync`). |
| `OXIDE_ASSETS_DIR` | autodetección (`./assets`) | Ruta explícita al directorio `assets/`. |

## Ejemplos

```powershell
# Reproducir Blammed en Hard con Botplay
oxide.exe --song:blammed --difficulty:hard --botplay

# Reproducir Bopeebo en Nightmare a 1.25x sin VSync
oxide.exe --song:bopeebo --diff:nightmare --rate:1.25 --no-vsync

# Ver ayuda de flags
oxide.exe --help
```

