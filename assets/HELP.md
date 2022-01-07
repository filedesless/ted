# Welcome to Ted

## NORMAL mode

In this mode keystrokes have a special meaning, strongly inspired by vim.

- `SPC q` to quit ted
- `SPC` to enter commands by chain

### Moving the cursor

These commands can be prefixed with a number to repeat the operation many times.

- `h, j, k, l` to move your cursor around in normal mode
- `J, K` to move a page up or down
- `H, L` to move beginning or end of line

### Enter INSERT mode

From INSERT mode, keystrokes are sent directly to the buffer to edit its content.
Return to NORMAL mode by pressing `ESC` or `Ctrl-c`

- `i, I` to insert under cursor or at beginning of line
- `a, A` to append after cursor or at end of line
- `o, O` to append newline under or above current line

### Edit the buffer

Some of these commands can optionally be prefixed with a number to repeat the operation many times.

- `d, D` to delete the n characters or lines under cursor
- `c, C` to copy the n characters or lines under cursor
- `p, P` to paste the character or line n times under cursor

### Text selection

Selecting text is achieved by marking a starting position or line, then moving the cursor to expand the selection. 

- `v, V` to select from the character or line under cursor (`ESC` or start a new selection to cancel)

## SPACE chains

Enter chains starting with `SPC` to run the following commands
