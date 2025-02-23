# A toy implementation of a small subset of git in Rust

This is my Rust solution to the codecrafters git challenge.

It's mostly just the minimum needed to pass the tests, but I slightly extended
the scope when I thought it would teach me something more about Rust.

While this is a toy implementation, I tried making the code clean, for a better
learning experience.

**Goals:**
- Realistic error handling: should not panic on things that can happen.
- Streaming read/write of files (avoid full copy in memory).
- No arbitrary limitations (eg, file names are allowed not to be UTF-8).
- Don't handle every corner case but try to detect things we don't handle.

**Non-goals:**
- Portability beyond Unix.
- Performance (beyond avoiding stupid things that would badly hurt it).
- Thread safety, concurrency safety (wrt another git process).
- Proper testing (unit testing, negative testing, etc.)

_Other educative implementations of subsets of git:_
- [My solution](https://github.com/mpg/codecrafters-git-python) to the same challenge in Python (and less clean).
- Jon Gjengset's solution to this challenge (except the clone stage) in Rust:
  [final code](https://github.com/jonhoo/codecrafters-git-rust) and
  [livestream replay](https://www.youtube.com/watch?v=u0VotuGzD_w) (super
  informative.
- A Haskell implementation of git clone
  [explained](https://stefan.saasen.me/articles/git-clone-in-haskell-from-the-bottom-up/)
  which provided the strategy I used for clone.
- A [different subset in Python](https://wyag.thb.lt): the index (staging area),
  more about references, creating commits, etc. but no networking.
- Yet [another subset in Python](https://benhoyt.com/writings/pygit/): the index
  (staging area), creating commits, pushing to a remote.


***
(Original codecrafter Readme below.)
***



[![progress-banner](https://backend.codecrafters.io/progress/git/be88b56c-60bd-49b5-899b-b569f58a5678)](https://app.codecrafters.io/users/codecrafters-bot?r=2qF)

This is a starting point for Rust solutions to the
["Build Your Own Git" Challenge](https://codecrafters.io/challenges/git).

In this challenge, you'll build a small Git implementation that's capable of
initializing a repository, creating commits and cloning a public repository.
Along the way we'll learn about the `.git` directory, Git objects (blobs,
commits, trees etc.), Git's transfer protocols and more.

**Note**: If you're viewing this repo on GitHub, head over to
[codecrafters.io](https://codecrafters.io) to try the challenge.

# Passing the first stage

The entry point for your Git implementation is in `src/main.rs`. Study and
uncomment the relevant code, and push your changes to pass the first stage:

```sh
git commit -am "pass 1st stage" # any msg
git push origin master
```

That's all!

# Stage 2 & beyond

Note: This section is for stages 2 and beyond.

1. Ensure you have `cargo (1.82)` installed locally
1. Run `./your_program.sh` to run your Git implementation, which is implemented
   in `src/main.rs`. This command compiles your Rust project, so it might be
   slow the first time you run it. Subsequent runs will be fast.
1. Commit your changes and run `git push origin master` to submit your solution
   to CodeCrafters. Test output will be streamed to your terminal.

# Testing locally

The `your_program.sh` script is expected to operate on the `.git` folder inside
the current working directory. If you're running this inside the root of this
repository, you might end up accidentally damaging your repository's `.git`
folder.

We suggest executing `your_program.sh` in a different folder when testing
locally. For example:

```sh
mkdir -p /tmp/testing && cd /tmp/testing
/path/to/your/repo/your_program.sh init
```

To make this easier to type out, you could add a
[shell alias](https://shapeshed.com/unix-alias/):

```sh
alias mygit=/path/to/your/repo/your_program.sh

mkdir -p /tmp/testing && cd /tmp/testing
mygit init
```
