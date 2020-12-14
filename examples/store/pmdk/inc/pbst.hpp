#include <iostream>
#include <libpmemobj++/make_persistent.hpp>
#include <libpmemobj++/p.hpp>
#include <libpmemobj++/persistent_ptr.hpp>
#include <libpmemobj++/pool.hpp>
#include <libpmemobj++/transaction.hpp>
#include <stdexcept>
#include <string>
#include <vector>
#include <iterator>
#include <sys/stat.h>
#include "prog.hpp"

using pmem::obj::delete_persistent;
using pmem::obj::make_persistent;
using pmem::obj::p;
using pmem::obj::persistent_ptr;
using pmem::obj::pool;
using pmem::obj::pool_base;
using pmem::obj::transaction;
using namespace std;

#define BST_LAYOUT "bst"
class pbst;
pool<pbst> pbst_pop;

class pbst {

	struct pmem_entry {
		persistent_ptr<pmem_entry> left;
		persistent_ptr<pmem_entry> right;
		p<uint64_t> value;
	};

public:
	pbst() { head = nullptr; }

	friend void perform_pbst(vector<string> args);
    
private:
    void insert_impl(persistent_ptr<pmem_entry> &node, uint64_t value) {
        if (node == nullptr) {
            transaction::run(pbst_pop, [&] {
                auto n = make_persistent<pmem_entry>();

                n->value = value;
                n->left = nullptr;
                n->right = nullptr;

                node = n;
            });
        } else if (value < node->value) {
            insert_impl(node->left, value);
        } else if (value > node->value) {
            insert_impl(node->right, value);
        }
    }

    persistent_ptr<pmem_entry> &largest(persistent_ptr<pmem_entry> &node) {
        if (node->right != nullptr) {
            return largest(node->right);
        } else {
            return node;
        }
    }

    persistent_ptr<pmem_entry> &smallest(persistent_ptr<pmem_entry> &node) {
        if (node->left != nullptr) {
            return smallest(node->left);
        } else {
            return node;
        }
    }

    void remove_impl(persistent_ptr<pmem_entry> &node, uint64_t value) {
        if (node == nullptr) {
            return;
        } else if (value == node->value) {
            transaction::run(pbst_pop, [&] {
                if (node->left == nullptr && node->right == nullptr) {
			        delete_persistent<pmem_entry>(node);
                    node = nullptr;
                } else if (node->left != nullptr) {
                    auto &succ = largest(node->left);
                    node->value = succ->value;
			        remove_impl(succ, succ->value);
                } else {
                    auto &succ = smallest(node->right);
                    node->value = succ->value;
			        remove_impl(succ, succ->value);
                }
            });
        } else if (value < node->value) {
            remove_impl(node->left, value);
        } else {
            remove_impl(node->right, value);
        }
    }

    persistent_ptr<pmem_entry> &search_impl(persistent_ptr<pmem_entry> &node,
        uint64_t value
    ) {
        if (node->value == value) {
            return node;
        } else if (value < node->value) {
            search_impl(node->left, value);
        } else if (value > node->value) {
            search_impl(node->right, value);
        }
    }

    string print_impl(persistent_ptr<pmem_entry> &node, string prefix,
        uint64_t *look
    ) {
        string res;
        if (node == nullptr) return "Empty\n";
		if (look == nullptr) {
            res = format("%lu\n", node->value);
        } else {
            if (node->value == *look) {
                res = format("\x1B[1;31m%lu\x1B[0m\n", node->value);
            } else {
                res = format("%lu\n", node->value);
            }
        }
        if (node->left == nullptr) {
            res += format("%s├─x\n", prefix.c_str());
        } else {
            res += format("%s├─ %s\n", prefix.c_str(),
                print_impl(node->left, prefix + "│  ", look).c_str());
        }
        if (node->right == nullptr) {
            res += format("%s└─x", prefix.c_str());
        } else {
            res += format("%s└─ %s", prefix.c_str(),
                print_impl(node->right, prefix + "   ", look).c_str());
        }
        return res;
	}

public:
	void insert(uint64_t value) {
		insert_impl(head, value);
	}

	void remove(uint64_t value) {
		remove_impl(head, value);
	}

    bool search(uint64_t value) {
        return search_impl(head, value) != nullptr;
    }

	void print(void) {
		cout << print_impl(head, "", nullptr) << endl;
	}

	void find(uint64_t value) {
		cout << search(value) << endl;
	}

    bool exec(vector<string> args) {
		if (args.size() < 2) {
            help();
        } else {
            int i = 2;
            while (i < args.size()) {
				string op = next_op(args, i);
                if (!op.empty()) {
                    if (op == "help") {
                        help();
                    } else if (op == "repeat") {
						string n = next_op(args, i);
                        if (!n.empty()) {
                            int m = std::stoi(n);
                            if (!repeat(args, i, m,[&](vector<string> args) {
                                return exec(args);
                            } )) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else if (op == "i") {
						string n = next_op(args, i);
                        if (!n.empty()) {
                            uint64_t m = std::stoull(n);
                            insert(m);
                        } else {
                            return false;
                        }
                    } else if (op == "r") {
						string n = next_op(args, i);
                        if (!n.empty()) {
                            uint64_t m = std::stoull(n);
                            remove(m);
                        } else {
                            return false;
                        }
                    } else if (op == "c") {
						string n = next_op(args, i);
                        if (!n.empty()) {
                            uint64_t m = std::stoull(n);
                            find(m);
                        } else {
                            return false;
                        }
                    } else if (op == "run") {
						string filename = next_op(args, i);
                        if (!filename.empty()) {
                            return run(filename, [&](vector<string> args){
                                return exec(args);
                            });
                        } else {
                            return false;
                        }
                    } else if (op == "print") {
                        print();
                    } else if (op == "help") {
                        help();
                    }
                } else {
                    return true;
                }
            }
        }
        return true;
	}

    void help() {
		std::cout << "usage: store vlist [OPERATIONS]" << std::endl
				  << "data type: uint64_t" << std::endl << std::endl
				  << "OPERATIONS:" << std::endl
        		  << "  ins data         Insert data" << std::endl
        		  << "  del data         Delete data" << std::endl
        		  << "  find data        Search for data" << std::endl
        		  << "  repeat n         Repeat the next operation n times" << std::endl
        		  << "  run file         Run a script file" << std::endl
        		  << "  clear            Delete all elements" << std::endl
        		  << "  print            Print the entire list" << std::endl
        		  << "  help             Display help" << std::endl;
	}

private:
	persistent_ptr<pmem_entry> head;
};

void perform_pbst(vector<string> args) {
	const char *path = "/mnt/pmem1/pmdk-bst.pool";
	persistent_ptr<pbst> self;

	try {
		if (file_exists(path) != 0) {
			pbst_pop = pool<pbst>::create(
				path, BST_LAYOUT, PMEMOBJ_MIN_POOL, CREATE_MODE_RW);
		} else {
			pbst_pop = pool<pbst>::open(path, BST_LAYOUT);
		}
		self = pbst_pop.root();
	} catch (const pmem::pool_error &e) {
		std::cerr << "Exception: " << e.what() << std::endl;
		exit(1);
	} catch (const pmem::transaction_error &e) {
		std::cerr << "Exception: " << e.what() << std::endl;
		exit(1);
	}

	self->exec(args);

	try {
		pbst_pop.close();
	} catch (const std::logic_error &e) {
		std::cerr << "Exception: " << e.what() << std::endl;
		exit(1);
	}
}
