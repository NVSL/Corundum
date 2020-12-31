package main

import (
	"flag"
	"os"
	"fmt"
	"strconv"
	"hash/fnv"

	"github.com/vmware/go-pmem-transaction/pmem"
	"github.com/vmware/go-pmem-transaction/transaction"
)

const N int = 10

type pair struct {
	key   [32]byte
	idx   int
}

type data struct {
	buckets [][]pair
	values  []int
	magic   int
}

const (
	// A magic number used to identify if the root object initialization
	// completed successfully.
	magic = 0x1B2E8BFF7BFBD154
)

func hash(s string) int {
	h := fnv.New32a()
	h.Write([]byte(s))
	return int(h.Sum32())
}

func initialize(ptr *data) {
	txn("undo") {
		ptr.buckets = pmake([][]pair, N)
		ptr.magic = magic
	}
}

func get(ptr *data, key string) *int {
	index := hash(key) % N
	var bytes [32]byte
	copy(bytes[:], key)

	for i:=0; i<len(ptr.buckets[index]); i++ {
		e := ptr.buckets[index][i]
		if e.key == bytes {
			return &ptr.values[e.idx]
		}
	}

	return nil
}

func put(ptr *data, key string, val int) {
	index := hash(key) % N
	var bytes [32]byte
	copy(bytes[:], key)

	txn("undo") {
		/* search for element with specified key - if found
		 * transactionally update its value */
		for i:=0; i<len(ptr.buckets[index]); i++ {
			e := ptr.buckets[index][i];
			if e.key == bytes {
				ptr.values[e.idx] = val
				return
			}
		}

		/* if there is no element with specified key, insert new value
		 * to the end of values vector and put reference in proper
		 * bucket transactionally */
		l1 := len(ptr.values)
		if len(ptr.values) == 0 {
			ptr.values = pmake([]int, 0, 1)
		}
		ptr.values = append(ptr.values, val)

		if len(ptr.buckets[index]) == 0 {
			ptr.buckets[index] = pmake([]pair, 0, 1)
		}
		ptr.buckets[index] = append(ptr.buckets[index], pair {bytes, l1})
	}
}

func show_usage(prog string) {
	println("usage:", prog, "filename [get key|put key value]")

}

func main() {
	args := os.Args

	if len(args) < 4 {
		show_usage(args[0])
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

	if args[2] == "get" && len(args) == 4 {
		if n := get(ptr, args[3]); n != nil {
			fmt.Println(*n)
		} else {
			fmt.Println("No value found for", args[3])
		}
	} else if args[2] == "put" && len(args) == 5 {
		if n, err := strconv.Atoi(args[4]); err == nil {
			put(ptr, args[3], n)
		}
	} else if args[2] == "burst" && args[3] =="get" && len(args) == 5 {
		if m, err := strconv.Atoi(args[4]); err == nil {
			var v *int
			for i := 0; i < m; i++ {
				key := fmt.Sprintf("key%d", i);
				v = get(ptr, key)
			}
			if v != nil {
				fmt.Println("v =", *v)
			}
		}
    } else if args[2] == "burst" && args[3] == "put" && len(args) == 5 {
		if m, err := strconv.Atoi(args[4]); err == nil {
			for i := 0; i < m; i++ {
				key := fmt.Sprintf("key%d", i);
				put(ptr, key, i);
			}
		}
    } else {
        show_usage(args[0]);
    }
}

