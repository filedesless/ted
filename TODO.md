# Todos

- [ ] undo stack (tree?)
- [ ] move by word / paragraph / page
- [ ] remember last col when being moved to left because line too short
- [ ] line wrapping
- [ ] line numbering
- [ ] highlight current line
  - [x] gotta rework buffer redraw
  - [ ] cache only annotated text rather than the escaped lines
    * to draw using TUI color
    * ideally also avoid allocating a bunch of `String`s in the cache
- [ ] make highlighting async
  - [x] don't have to re-highlight everything on every change to the buffer  (https://github.com/trishume/syntect#caching)
- [-] visual mode
  - [~] highlight selected text
    - died when integrating syntect
  - [ ] re-implement commands to work on selected text
- [x] handle buffers bigger than the screen
  - [x] doesn't panic anymore
- [x] dirty/must_redraw bit on buffers
- [x] paste buffer
- [x] numeric modifier for normal mode commands
- [x] load/save from file
- [x] space prompt
- [x] only display visible window
