# lddcheck

## Description
This is a cross-platform tool that checks the minimum required glibc version for given linux binaries. It is useful for the developers who want to know the minimum required glibc version for their prebuilt binaries. It is also useful for the system administrators who want to know the minimum required glibc version for the binaries they want to install.

## Usage
```bash
$ lddcheck --help
Usage: lddcheck [OPTIONS] --paths <PATHS>

Options:
  -p, --paths <PATHS>
          The path to the binary to analyze
      --root <ROOT>
          The root path to use when resolving paths [default: /]
  -l, --ld-library-path <LD_LIBRARY_PATH>
          Additional LD_LIBRARY_PATH to use when resolving paths
  -s, --scope <SCOPES>
          Only consider libraries under these paths [default: /]
      --stdout <STDOUT_FORMAT>
          The format to use when printing to stdout [default: text] [possible values: json, text]
      --save-json-to <SAVE_JSON_TO>
          Save the json to a file
      --pretty-json
          Pretty print the json
      --versions <VERSIONS>
          The number of highest required glibc versions to print [default: 1]
      --detail-level <DETAIL_LEVEL>
          The detail level to use when printing to stdout [default: version] [possible values: version, function, file]
      --print-error <PRINT_ERROR>
          If and what errors to print to stderr [default: all] [possible values: cannot-parse, cannot-read, not-found, none, all]
  -h, --help
          Print help
  -V, --version
          Print version
```

If the binary does not require any glibc version, the tool will not print anything to stdout (note that you might still get output on stderr!) and will exit with code 0 (unless any given binaries cannot be read or parsed correctly).
