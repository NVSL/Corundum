package main

import (
	"flag"
	"os"
	"strconv"

	"github.com/vmware/go-pmem-transaction/pmem"
	"github.com/vmware/go-pmem-transaction/transaction"
)

type node struct {
	key   int
	value [32]byte
	slots [2]*node
}

type data struct {
	root  *node
	magic int
}

const (
	// A magic number used to identify if the root object initialization
	// completed successfully.
	magic = 0x1B2E8BFF7BFBD154
)

func initialize(ptr *data) {
	txn("undo") {
		ptr.root = nil
		ptr.magic = magic
	}
}

func insert(ptr **node, key int, value string) {
	if *ptr == nil {
		txn("undo") { 
			*ptr = pnew(node)
			(*ptr).key = key
			copy((*ptr).value[:], value)
		}
	} else {
		i := 0
		if key > (*ptr).key {
			i = 1
		}
		insert(&(*ptr).slots[i], key, value)
	}
}

func find(ptr *node, key int) *node {
	if ptr == nil {
		return nil
	} else if ptr.key == key {
		return ptr
	} else {
		i := 0
		if key > (*ptr).key {
			i = 1
		}
		return find(ptr.slots[i], key)
	}
}

func print_node(ptr *node) {
	if ptr != nil {
		print_node(ptr.slots[0])
		print(string(ptr.value[:]), " ")
		print_node(ptr.slots[1])
	}
}

func main() {
	args := os.Args

	if len(args) < 3 {
		println("usage:", args[0], "filename [p|i|f|s|r] [key] [value]")
		return
	}

	var ptr *data
	flag.Parse()
	firstInit := pmem.Init(args[1])
	if firstInit {
		// first time run of the application
		ptr = (*data)(pmem.New("root", ptr))
		initialize(ptr)
	} else {
		// not a first time initialization
		ptr = (*data)(pmem.Get("root", ptr))

		// even though this is not a first time initialization, we should still
		// check if the named object exists and data initialization completed
		// succesfully. The magic element within the named object helps check
		// for successful data initialization.

		if ptr == nil {
			ptr = (*data)(pmem.New("root", ptr))
		}

		if ptr.magic != magic {
			initialize(ptr)
		}
	}
	op := args[2][0]
	switch op {
	case 'p':
		print_node(ptr.root)
		println()
	case 'i':
		if key, err := strconv.Atoi(args[3]); err == nil {
			value := args[4]
			insert(&ptr.root, key, value)
		}
	case 'f':
		if key, err := strconv.Atoi(args[3]); err == nil {
			p := find(ptr.root, key)
			if p != nil {
				println(string(p.value[:]))
			} else {
				println("not found")
			}
		}
	case 's':
		if len, err := strconv.Atoi(args[3]); err == nil {
			for k := 0; k < len; k++ {
				insert(&ptr.root, k, "test")
			}
		}
	case 'r':
		if len, err := strconv.Atoi(args[3]); err == nil {
			var p *node = nil
			for k := 0; k < len; k++ {
				p = find(ptr.root, k)
			}
			if p != nil {
				println("value = ", string(p.value[:]))
			}
		}
	default:
		println("invalid operation")
	}
}

