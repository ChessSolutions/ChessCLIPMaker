# ChessClipMaker

ChessClipMaker turns games from Chess.com, Lichess, or a pasted PGN into
custom animated GIFs.

I wanted a simple way to make a GIF from only the important part of a chess
game instead of exporting every move. ChessClipMaker lets you choose a specific
move range, add a title screen, insert pictures or animated GIFs, write formatted
captions, and arrange everything on an editable timeline before exporting it.

The project is based on the excellent
[lila-gif](https://github.com/lichess-org/lila-gif) renderer and expands it with
an editor, game importing, media frames, captions, player information, and a
complete preview-and-download workflow.

## What it can do

- Paste a PGN or import a game using a Chess.com or Lichess game URL.
- Enter a saved player handle and load any of their last 10 games.
- Keep separate lists of Chess.com and Lichess players and select a default
  player.
- Automatically orient the board from the imported player's side.
- Select only the moves you want in the finished animation.
- Adjust every frame's duration in milliseconds.
- Reorder frames on the timeline or change their frame numbers and sort them.
- Generate a title frame with player names, ratings, date, and playing site.
- Show usernames, ratings, and available PGN clock times around the board.
- Add formatted caption frames with Google Fonts, text color, background color,
  alignment, size, and padding controls.
- Add a picture or another animated GIF, resize it, reposition it, and place it
  anywhere in the timeline.
- Change the dark-square color, coordinates, move highlights, and board
  orientation.
- Preview the complete animation before downloading the final GIF.

## Typical workflow

1. Paste a PGN, game URL, or saved player handle.
2. When using a player handle, choose their latest game or one of the previous
   nine games.
3. Click **Import** to parse the game, build the timeline, and generate a
   preview.
4. Choose a move range and customize individual frame durations.
5. Add or edit the title, captions, pictures, and GIF frames.
6. Reorder the timeline and choose the board orientation and display options.
7. Preview the result, then download the GIF.

## Running from source

Install Rust, clone the repository, and run:

```sh
cargo run --release
```

ChessClipMaker listens on `127.0.0.1:6175` and opens the editor in your default
browser. To use a different address:

```sh
cargo run --release -- --bind 127.0.0.1:7000
```

## Windows executable

The Windows build is a single, portable `ChessClipMaker.exe`. The editor web
interface is embedded in the executable. Double-clicking it starts the local
server and opens the editor in the default browser. Saved accounts are written
to `accounts.json` beside the executable.

To create it on GitHub:

1. Open the repository's **Actions** tab.
2. Select **Build Windows EXE**.
3. Choose **Run workflow**.
4. Download the **ChessClipMaker-Windows** artifact after the job finishes.
5. Unzip it and share `ChessClipMaker.exe`.

Windows may show a SmartScreen warning because privately distributed builds are
not code-signed. A commercial code-signing certificate is required to remove
that warning reliably.

## Roadmap

- Add arrows, circles, colored squares, and other manual chess annotations.
- Add reusable annotation frames for explaining tactics and plans.
- Add a Stockfish evaluation bar beside the board.
- Automatically identify blunders, mistakes, inaccuracies, and exceptional
  moves, then offer to insert an explanation or annotation frame after them.
- Add optional engine lines and short variations to analysis frames.

### Proposed Stockfish approach

Stockfish analysis should happen when a PGN is imported, not while GIF pixels
are being encoded. A practical implementation would:

1. Start Stockfish as a child process and communicate with it using the UCI
   protocol.
2. Send each timeline position to the engine with `position fen ...`.
3. Analyze each position to a configurable depth or time limit using a command
   such as `go depth 16` or `go movetime 300`.
4. Store the returned centipawn or mate score on the corresponding chess frame.
5. Normalize that score into a visual evaluation-bar percentage while preserving
   mate scores as decisive positions.
6. Compare the evaluation before and after each played move from the moving
   player's perspective.
7. Use configurable thresholds to classify evaluation loss. For example, a
   large loss could be marked as a blunder, while a move matching the engine's
   best move in a difficult position could be considered a great move.
8. Insert suggested annotation frames into the timeline, allowing the user to
   edit, move, or delete them before export.

Engine analysis should be cached by FEN and settings so rebuilding a GIF does
not repeatedly analyze the same positions. The first version should make all
labels optional because terms such as “great move” require more context than a
single centipawn threshold. Stockfish would also need to be bundled with desktop
releases or selected by the user, and its GPL license and source-distribution
requirements must be respected.

## HTTP API

### `GET /image.gif`

```
curl "http://localhost:6175/image.gif?fen=4k3/6KP/8/8/8/8/7p/8" --output image.gif
```

| name        | type  | default                                   | description                                                                                  |
| ----------- | ----- | ----------------------------------------- | -------------------------------------------------------------------------------------------- |
| **fen**     | ascii | _starting position_                       | FEN of the position. Board part is sufficient.                                               |
| white       | utf-8 | _none_                                    | Name of the white player. Known chess titles are highlighted. Limited to 100 bytes.          |
| black       | utf-8 | _none_                                    | Name of the black player. Known chess titles are highlighted. Limited to 100 bytes.          |
| comment     | utf-8 | `https://github.com/lichess-org/lila-gif` | Comment to be added to GIF meta data. Limited to 255 bytes.                                  |
| lastMove    | ascii | _none_                                    | Last move in UCI notation (like `e2e4`).                                                     |
| check       | ascii | _none_                                    | Square of king in check (like `e1`).                                                         |
| orientation |       | `white`                                   | Pass `black` to flip the board.                                                              |
| theme       |       | `brown`                                   | Board theme. `blue`, `brown`, `green`, `ic`, `pink`, or `purple`.                            |
| piece       |       | `cburnett`                                | Piece set from this [list](https://github.com/lichess-org/lila-gif/tree/master/theme/piece). |

### `POST /game.gif`

```javascript
{
  "white": "Molinari", // optional
  "black": "Bordais", // optional
  "comment": "https://www.chessgames.com/perl/chessgame?gid=1251038", // optional
  "orientation": "white", // default
  "theme": "brown", // default
  "piece": "cburnett", // default
  "delay": 50, // default frame delay in centiseconds
  "frames": [
    // [...]
    {
      "fen": "r1bqkb1r/pp1ppppp/5n2/2p5/2P1P3/2Nn2P1/PP1PNP1P/R1BQKB1R w KQkq - 1 6",
      "delay": 500, // optionally overwrite default delay
      "lastMove": "b4d3", // optionally highlight last move
      "check": "e1" // optionally highlight king
    }
  ]
}
```

### `GET /example.gif`

```
curl http://localhost:6175/example.gif --output example.gif
```

Render an [example game](https://lichess.org/Q0iQs5Zi).

## Technique

Instead of rendering vector graphics at runtime, all pieces are prerendered
on every possible background. This allows preparing a minimal color palette
ahead of time. (Pieces are not just black and white, but need other colors
for anti-aliasing on the different background colors).

![Sprite](/theme/sprites/brown-cburnett.gif)

All that's left to do at runtime is copy sprites and encode the GIF.
More than 95% of the rendering time is spent in LZW compression.

For animated games, frames only contain the changed squares on transparent
background. The example below is the last frame of the animation.

![Example frame](/example-frame.gif)

## License

ChessClipMaker is derived from lila-gif and is licensed under the GNU Affero
General Public License, version 3 or any later version, at your option.

The generated images include text in
[Noto Sans](https://fonts.google.com/specimen/Noto+Sans) (Apache License 2.0)
and a pieces sets with
[various licenses](https://github.com/lichess-org/lila/blob/master/COPYING.md).
