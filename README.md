# mediavert

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/mediavert-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/mediavert)
[<img alt="crates.io" src="https://img.shields.io/crates/v/mediavert.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/mediavert)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-mediavert-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/mediavert)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/mediavert/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/mediavert/actions?query=branch%3Amain)

A tool to perform batch conversion of media.

This combines several tools as subcommands:
* [`bookvert`] ([git][bookvert-git]) - `convert books` which is a tool to convert
  directories of images into `.cbz` books.
* [`audiovert`] ([git][audiovert-git]) - `convert music` which is a tool to convert
  tagged or untagged music from one format and directory structure to
  another.

<br>

## Examples

You can run the included example like this:

```sh
mediavert audio --meta -D examples/unsorted --to examples/sorted
```

[`bookvert`]: https://crates.io/crates/bookvert
[bookvert-git]: https://github.com/udoprog/mediavert/tree/main/crates/bookvert
[`audiovert`]: https://crates.io/crates/audiovert
[audiovert-git]: https://github.com/udoprog/mediavert/tree/main/crates/audiovert
