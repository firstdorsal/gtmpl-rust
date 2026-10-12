//go:build ignore

// Generates the Go side of go_conformance.txt.
//
// Reads one template per line on stdin (with "\n" escaped) and writes the template, a
// tab, then OK and the rendered output as a quoted string, or an error marker. Run it
// against the fixture's first column to see what real Go does with each case:
//
//	awk -F'\t' '!/^#/ && NF>0 {print $1}' go_conformance.txt |
//	    go run go_conformance.go > /tmp/go-side.txt
//
// The fixture records what *this engine* must produce, which is Go's output except for
// the deviations its header lists; diffing the two shows exactly those.
package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
	"text/template"
)

func main() {
	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 1024*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if line == "" {
			continue
		}
		source := strings.ReplaceAll(line, "\\n", "\n")
		tmpl, err := template.New("t").Parse(source)
		if err != nil {
			fmt.Printf("%s\tERR\t%s\n", line, err)
			continue
		}
		var out strings.Builder
		if err := tmpl.Execute(&out, nil); err != nil {
			fmt.Printf("%s\tERR\t%s\n", line, err)
			continue
		}
		fmt.Printf("%s\tOK\t%q\n", line, out.String())
	}
}
