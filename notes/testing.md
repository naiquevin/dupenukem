Testing dupenukem
=================

Unit tests
----------

To run unit tests, you can simply run,

```bash
    cargo test
```

End-to-end tests
----------------

For end to end testing, a utility script [ffix](../scripts/ffix) is
provided which helps in creating a directory with duplicate files. It
takes a bash script as one of the argument inside which you can write
any commands for creating the necessary dir structure. Let's call it a
"fixture script". An example fixture script
[basic.sh](../examples/fixtures/basic.sh) is provided.

Example usage:

```bash
    scripts/ffix .testdirs/one examples/fixtures/basic.sh
    Creating .testdirs/one
    Changing directory to .testdirs/one
    Executing the script: /Users/vineet/code/dupenukem/examples/fixtures/basic.sh
    Created the following directory structure in .testdirs/one
    .testdirs/one
    |-- bar
    |   |-- 1.txt
    |   `-- 2.txt
    |-- cat
    |   |-- 1.txt
    |   `-- bar_one.txt -> ../bar/1.txt
    `-- foo
        |-- 1.txt
        `-- xx.txt -> /tmp/xx.txt

    4 directories, 6 files
```

Refer to the source code/documentation of that file for more details

Now you can invoke the dupenukem command using cargo as follows,

```bash
    cargo run -- find .testdirs/one --quick
```

To test other use cases, you may add more "fixture scripts" under the
examples directory.

Make sure that the test directories created by ffix are not getting
committed to the git repo.
