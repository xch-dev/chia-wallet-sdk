// Command modzip creates the zip that the Go module proxy would serve for a release of the
// module, which fails if the module is over the proxy's size limits.
//
// Usage: go run . <module-dir> <module-path> <version> <output-zip>
package main

import (
	"fmt"
	"os"

	"golang.org/x/mod/module"
	"golang.org/x/mod/zip"
)

func main() {
	if len(os.Args) != 5 {
		fmt.Fprintln(os.Stderr, "usage: modzip <module-dir> <module-path> <version> <output-zip>")
		os.Exit(2)
	}
	if err := run(os.Args[1], os.Args[2], os.Args[3], os.Args[4]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func run(dir, path, version, out string) error {
	f, err := os.Create(out)
	if err != nil {
		return err
	}
	defer f.Close()

	if err := zip.CreateFromDir(f, module.Version{Path: path, Version: version}, dir); err != nil {
		return err
	}

	info, err := f.Stat()
	if err != nil {
		return err
	}
	fmt.Printf("%s@%s is %.1f MiB zipped (limit %d MiB)\n", path, version, float64(info.Size())/(1<<20), zip.MaxZipFile>>20)
	return nil
}
