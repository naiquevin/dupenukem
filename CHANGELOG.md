Changelog
=========

0.2.0 (unreleased)
------------------

- Duplicate groups in the snapshot output are now sorted in descending
  order of the size i.e. larger files will appear first in the
  list. May help the user prioritize the deduplication

- The find command now logs the maximum amount of space that can be
  freed up by deduplication

- The `execute` command also outputs the amount of space that's
  actually freed up (or will free up, in case of dry-run

- Code has been refactored to use `&Path` instead of `&PathBuf`
  wherever an owned type is not required

- The `inquire` crate is upgraded to version `0.7.0`

- Github workflow for publishing releases implemented.
