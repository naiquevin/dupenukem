Dupenukem
=========

Dupenukem is a simple command line utility for file deduplication.

:warning: Warning
-----------------

This is a personal project for learning and experimenting with the
[Rust programming language](https://www.rust-lang.org/). It doesn't
claim to be fast or efficient by any means. It doesn't support Windows
(at present). Moreover, it's designed to perform destructive
operations such as deleting files from your computer. Please use with
caution.

If you're looking for a serious file deduplication software there is
[fclones](https://github.com/pkolaczk/fclones) which is highly
performant and popular. There must be other alternatives too.

Having said that, I've used `dupenukem` to clean my Dropbox folder and
a couple of external hard drives. I plan to ship features and
improvements based on my use case, or just use as an opportunity to
code in rust.

It has been tested only on MacOS, although it should theoretically
work on Linux too. As I don't have access to a Windows machine, there
is no plan to support Windows at least in the near future.

Installation
------------

I am still figuring out how to use github workflows for building and
distributing binaries. In the meanwhile, you can install it using
`cargo`, directly from github,

``` shell
    cargo install --git https://github.com/naiquevin/dupenukem.git
```

Or build from source again, using `cargo`.

``` shell
    git clone git@github.com:naiquevin/dupenukem.git
    cd dupenukem
    cargo build --release

    # Copy the binary to some dir in your PATH
    cp target/release/dupenukem ~/bin

    # You can now run it
    dupenukem --help
```

Usage
-----

`dupenukem` provides three commands for a three step deduplication
workflow:

### Step 1: Finding duplicates and generating a snapshot

The `find` command accepts a `rootdir` and finds all duplicate files
under it. The output is what is called a "snapshot". This is nothing
but text representation of the state of duplicate files inside the
directory captured at that moment. This output is printed to stdout
and users must store it inside a file.

The snapshot format is explained in detail later in the example
section.

### Step 2: Editing the snapshot and validating changes

Once the snapshot file is generated, the user is supposed to edit it
in order to tell this tool what should be done with the duplicate
files. Only 2 options are currently supported:

1. duplicate files can be marked for deletion
2. duplicate files can be marked for symlinking i.e. a duplicate file
   will be replaced with a symlink to an original one (can be decided
   by the user)

An updated snapshot can be validated using the `validate` command
which basically checks for compatibility of the snapshot and the
changes w.r.t the current state of the files. This is to protect
against data loss in case any changes get made to a previously
identified duplicate file.

### Step 3: Applying the changes

Once a user-edited snapshot has been validated it can be given as
input to the `apply` command, which will actually execute the
actions. The apply command also implicitly runs the `validate` step
again considering the time-of-check to time-of-use (TOCTOU) nature of
the workflow.

As it performs destructive operations, two safeguards are implemented:

1. The `apply` command can be run with a `--dry-run` flag which will
   cause all actions to be only logged and not actually executed. When
   run without the `--dry-run` flag, the user is also asked for
   `yes/no` confirmation to proceed.

2. Before deleting a file or replacing it with symlink, a backup is
   taken at another location (preserving the original directory
   structure). The user may delete the backup directory after
   verifying the actual changes performed on disk.

The apply command is also idempotent i.e. if run multiple times, the
already applied changes will be skipped. More accurately, the `apply`
command tries to get the files into the intended state indicated by
the action marker. If a file is already in that state, it will no-op
and move on. This way, the user may incrementally fix and verify one
group of duplicates or even one file at a time.

Example
-------

It's easier to explain the usage in detail with the help of an
example. For that let's first create a dummy directory with a few
duplicate files.

``` shell
    mkdir ~/dpnktest
    cd ~/dpnktest
    mkdir foo bar cat

    echo ONE > foo/1.txt
    cp foo/1.txt bar/

    echo TWO > foo/2.txt
    cp foo/2.txt cat

    echo THREE > foo/3.txt
    echo FOUR > bar/4.txt
```

This resulting dir structure will be:

``` shell
    $ tree --charset=ascii
    .
    |-- bar
    |   |-- 1.txt
    |   `-- 4.txt
    |-- cat
    |   `-- 2.txt
    `-- foo
        |-- 1.txt
        |-- 2.txt
        `-- 3.txt

    4 directories, 6 files
```

Now let's use `dupenukem` to find and fix duplicates inside this root
directory. It's assumed that the user running `dupenukem` has the
permissions to read and write files inside the root directory.

We'll begin by running the `find` command:

``` text
    $ dupenukem find ~/dpnktest | tee ~/dpnktest_snapshot.txt
    #! Root Directory: /Users/vineet/dpnktest
    #! Generated at: Tue, 16 Jan 2024 12:00:05 +0530

    [13062064944137093030]
    keep cat/2.txt
    keep foo/2.txt

    [10098984572146910405]
    keep foo/1.txt
    keep bar/1.txt

    # Reference:
    # keep <target> = keep the target path as it is
    # delete <target> = delete the target path
    # symlink <target> [-> <src>] = Replace target with a symlink
    # .       If 'src' is specified, it can either be an absolute or
    # .       relative (to 'target'). Else one of the duplicates marked
    # .       as 'keep' will be considered. If 'src' is not specified,
    # .       a relative symlink will be created.
    #
    # This section is a comment and will be ignored by the tool
```

Things to note:

- Two groups of duplicate files have been found. Each group has a
  unique identifier - `13062064944137093030` and
  `10098984572146910405`. These are nothing but 64-bit
  [xxhash3](https://xxhash.com/) hashes of the contents of the files.

- Under every group (indicated by the hash within square brackets),
  duplicate files in that group are listed along with an "action
  marker" which currently says `keep` for the files. Note that the
  file paths are relative to the root directory.

- The snapshot only contains duplicate files. For e.g. the files
  `foo/3.txt` and `bar/4.txt` have no duplicates so they are not
  included in the snapshot. Also, only the duplicate files located
  under the root dir are considered. Eg. If `bar/4.txt` happens to be
  a copy of `~/some/other/root/dir/4.txt`, it will still be excluded
  from the snapshot.

- At the beginning of the output, there are a couple of lines prefixed
  with `#!`, which are for storing/defining metadata. Users must not
  modify these lines.

- Near the end of the output there is a block of text with all lines
  prefixed with `#`. These are comments. The snapshot includes a
  simple reference for the action markers that the user may use when
  editing the file.

- Finally, we've redirected the (std) output to the file
  `~/dpnktest_snapshot.txt` in order to store the snapshot.

Now let's ask `dupenukem` to fix the duplicates as follows,

1. delete `cat/2.txt`
2. replace `bar/1.txt` with a symlink that points to `foo/1.txt`

To do that we'll edit the file as follows (excluding metadata and
comments for brevity):

``` text
    [..snip..]

    [13062064944137093030]
    delete cat/2.txt
    keep foo/2.txt

    [10098984572146910405]
    keep foo/1.txt
    symlink bar/1.txt

    [..snip..]
```

After making the above changes, we should validate the snapshot file.

``` shell
    $ dupenukem validate ~/dpnktest_snapshot.txt
    Snapshot is valid!
    No. of pending action(s): 2
```

Before proceeding with the `apply` command, let's consider the case
where some other process modifies the `bar/1.txt` file in the
meanwhile. Then the `validate` command would fail as `bar/1.txt` would
no longer be a duplicate of `foo/1.txt`.

However in this example, the snapshot is valid and there are 2 pending
actions to be performed. Before actually executing these actions we
can run the `apply` command with `--dry-run` flag to see what exactly
will happen:

``` shell
    $ dupenukem apply --dry-run ~/dpnktest_snapshot.txt
    [DRY RUN] File to be replaced with symlink: bar/1.txt -> ../foo/1.txt
    [DRY RUN] File to be deleted: cat/2.txt
    [DRY RUN] Backup will be stored under /Users/vineet/.dupenukem/backups
```

Notice the last line that mentions the backup location inside
`~/.dupenukem/backups`. It's assumed that the current user has
permissions to write to this location. Backups will be taken inside a
new directory under this location, with the dir name derived from the
current timestamp. This will ensure that multiple backups can
coexist. This also implies that it's up to the user to cleanup older
backups that are no longer required. The user can also choose to
override the backup directory by specifying the `--backup-dir` option.

Let's now proceed with running the `apply` command without the
`--dry-run` flag.

``` shell
    $ dupenukem apply ~/dpnktest_snapshot.txt
    > All changes will be executed. Do you want to proceed? Yes
```

This command doesn't print any output but it will ask for confirmation
before executing the actions. Let's inspect the directory structure
now using the same `tree` command:

``` shell
    $ cd ~/dpnktest
    $ tree --charset=ascii
    .
    |-- bar
    |   |-- 1.txt -> ../foo/1.txt
    |   `-- 4.txt
    |-- cat
    `-- foo
        |-- 1.txt
        |-- 2.txt
        `-- 3.txt

    4 directories, 5 files
```

And the desired changes can be seen.

The backup can be found under the default backup directory
`~/.dupenukem/backups`.

``` shell
    $ tree --charset=ascii ~/.dupenukem/backups
    /Users/vineet/.dupenukem/backups
    `-- 20240116160509
        |-- bar
        |   `-- 1.txt
        `-- cat
            `-- 2.txt

    4 directories, 2 files
```

Notice the dir name derived from timestamp and that the directory
structure is preserved. After verifying the changes, if the user
wishes to restore any files, it can be done easily. If everything
looks good, they may easily delete the backup dir
`~/.dupenukem/backups/20240116160509`.

The `apply` command is idempotent i.e. if we try running the `apply`
command once again, it will no-op.

Now let's see what happens if we run the `find` command once again on
the current state of the `~/dpnktest` directory.

``` text
    $ dupenukem find ~/dpnktest
    #! Root Directory: /Users/vineet/dpnktest
    #! Generated at: Thu, 18 Jan 2024 10:40:23 +0530

    [10098984572146910405]
    keep foo/1.txt
    symlink bar/1.txt -> ../foo/1.txt

    # Reference:
    # keep <target> = keep the target path as it is
    # delete <target> = delete the target path
    # symlink <target> [-> <src>] = Replace target with a symlink
    # .       If 'src' is specified, it can either be an absolute or
    # .       relative (to 'target'). Else one of the duplicates marked
    # .       as 'keep' will be considered. If 'src' is not specified,
    # .       a relative symlink will be created.
    #
    # This section is a comment and will be ignored by the tool
```

This time, it found only 1 group of 2 duplicate files among which one
is already a symlink to the other.

Symlink preferences
-------------------

### Implicit v/s Explicit symlink source paths

In the above example, we saw that to replace a file with a symlink we
added the `symlink` marker. On running the `apply` command,
`bar/1.txt` was replaced with a symlink pointing to `foo/1.txt`.

This means `dupenukem` will use use the other duplicate file marked as
`keep` as the symlink source path. But what if more than two
duplicates are found, out of which 2 of them are marked as `keep`?
Consider the following example:

``` text
    [..snip..]

    [10098984572146910405]
    keep foo/1.txt
    symlink bar/1.txt
    keep cat/one.txt

    [..snip..]
```

In this case, `dupenukem` will take the first entry from
lexicographically sorted list of all files marked with `keep`. That
would be `cat/one.txt` in case of this example.

Suppose the user wants that the symlink source path for `bar/1.txt`
should be `foo/1.txt` instead, they can explicitly mention it as
follows,

``` text
    [..snip..]

    [10098984572146910405]
    keep foo/1.txt
    symlink bar/1.txt -> ../foo/1.txt
    keep cat/one.txt

    [..snip..]
```

Note that the explicitly mentioned source path is relative to the
symlink (target) and not relative to the root directory.

### Relative v/s absolute symlinks

For most use cases, relative symlinks are desirable. Hence the default
behaviour (in case of implicit symlinks) is to use relative source
paths. But absolute symlinks are also supported - the user just needs
to explicitly specify the absolute source path, similar to the
previous example:

``` text
    [..snip..]

    [10098984572146910405]
    keep foo/1.txt
    symlink bar/1.txt -> /Users/vineet/dpnktest/foo/1.txt
    keep cat/one.txt

    [..snip..]
```

On running apply, `bar/1.txt` will be replaced with a symlink to the
absolute source path.

``` shell
    $ cd ~/dpnktest
    $ readlink bar/1.txt
    /Users/vineet/dpnktest/foo/1.txt
```

Exclusions
----------

Basic file exclusions by exact path are supported with the `--exclude`
flag. For example, when used to scan the Dropbox folder, it makes
sense to exclude the drop cache directories.

``` shell
    $ dupenukem find --exclude .dropbox.cache ~/Dropbox
```

How are duplicate files identified?
-----------------------------------

`dupenukem` recursively traverses the root directory (in breadth-first
manner) and then finds duplicate files in 3 steps:

1. First the file sizes are compared. All files with unique sizes are
   discarded and only the rest go through to the next step. The
   assumption is that duplicate files will have same sizes. As the
   sizes are obtained from file metadata, this step is extremely fast
   and significantly reduces the IO in the next step.

2. In this step, files are grouped by 64-bit `xxh3` hashes of the file
   content. The `xxh3` hashes are also used as the group identifiers
   in the snapshot output.

3. In the last step, it confirms that all files in a group
   (i.e. having same xxh3 hashes) have the same `sha256` hashes as
   well. This confirmation is optional but enabled by default. To
   disable it, the `--quick` flag can be used with the `find` command.

Future improvements
-------------------

- Improve the `exclude` functionality - support exclusions based on
  glob/patterns as well as min/max sizes (similar to rsync)
- Use async programming where applicable
- Add support for hardlinks
- Add commands backup management - restoring, clean up etc.
- May be support Windows at some point

License
-------

MIT (See [LICENSE](LICENSE)).
