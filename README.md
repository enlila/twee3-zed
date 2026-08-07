# Twee 3 (SugarCube) Extension for Zed

Welcome to the ultimate Twee 3 and SugarCube 2.0 development experience in Zed! This extension provides rich syntax highlighting, deep JavaScript integration, and seamless Tweego playtesting for your Twine projects.

## Features

- **Twee 3 & SugarCube 2.0 Syntax Highlighting**: Full support for variables, macros, links, styles, and all SugarCube specific operators.
- **Embedded Languages**: Passages tagged with `[script]` or `[stylesheet]` automatically use Zed's native JavaScript and CSS syntax highlighting.
- **Smart Autocomplete**: Context-aware completions for all passage names when you type `[[`, all workspace variables when you type `$`, and standard SugarCube 2 macros (like `<<set>>`, `<<if>>`).
- **Rich Hover Previews**: Hover over any passage link (`[[Passage]]`) to see a quick preview of its content without switching files, or hover over SugarCube macros to see documentation.
- **Robust Diagnostics**: Instantly see warning squiggles for broken links pointing to non-existent passages, and error indicators for duplicate passage headers.
- **Seamless Navigation**: `ctrl-click` (or `cmd-click`) on any passage link to instantly jump to its definition. Utilize Zed's Outline view to see all passages in the current file, or use global symbol search to find passages across your workspace.
- **Workspace Rename**: Easily rename a passage header and the extension will automatically update all references to that passage across your entire project.
- **Dynamic TypeScript Types**: When you open a Twee project, the extension seamlessly downloads `@types/twine-sugarcube` via NPM. Your `.js` files instantly get rich hover documentation and TypeScript autocomplete for SugarCube APIs!
- **Custom Macro Autocomplete**: The built-in Language Server (LSP) continuously scans your workspace for `Macro.add(...)` definitions. Your custom macros are automatically injected into the autocomplete dropdown inside `.twee` files!
- **Tweego Playtesting**: The extension automatically downloads the Tweego compiler. A `▶ Run Passage` button (Code Lens) will appear above every `:: Passage` header. Clicking it builds and opens that specific passage in your web browser!

## Project Structure & Configuration

To take full advantage of the Tweego playtesting feature, you should define a `twee-config.yaml` file in the root of your project. 

The Language Server reads this configuration to determine how to build your game when you click the `▶ Run Passage` button.

### Example `twee-config.yaml`

```yaml
# The Twine story format to use
storyFormat: sugarcube-2

# The directory containing your .twee source files
sourceDir: src

# The output HTML file path
outputFile: dist/game.html

# Additional directories to include as modules (e.g., scripts, CSS, assets)
modules:
  - script
  - stylesheet
  - assets

```

## How It Works Under the Hood

- **Tree-sitter**: We use a custom Tree-sitter grammar to accurately parse the complex nesting of SugarCube macros and standard Markdown.
- **LSP (`twee3-lsp`)**: A lightweight, blazing-fast Rust Language Server handles the intelligent features. It automatically fetches the latest Tweego binaries and typings, scans your workspace, and provides the Code Lenses for playtesting.
- **Zero Configuration**: Besides setting up your `twee-config.yaml` for Tweego, everything else works out of the box. No need to manually install Tweego or type definitions.

Enjoy building your interactive fiction!
