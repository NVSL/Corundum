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

#define LIST_LAYOUT "list"
class plist;
pool<plist> plist_pop;

class plist {

	struct pmem_entry {
		persistent_ptr<pmem_entry> next;
		p<uint64_t> value;
	};

public:
	plist() { head = nullptr; }

	friend void perform_plist(vector<string> args);

	void push_front(uint64_t value) {
		transaction::run(plist_pop, [&] {
			auto n = make_persistent<pmem_entry>();

			n->value = value;
			n->next = head;

			head = n;
		});
	}

	uint64_t pop_front() {
		uint64_t ret = 0;
		transaction::run(plist_pop, [&] {
			if (head == nullptr)
				transaction::abort(EINVAL);

			ret = head->value;
			auto n = head->next;

			delete_persistent<pmem_entry>(head);
			head = n;
		});

		return ret;
	}

	void push_back(uint64_t value) {
		transaction::run(plist_pop, [&] {
			auto n = make_persistent<pmem_entry>();

			n->value = value;
			n->next = nullptr;

			if (head == nullptr) {
				head = n;
			} else {
				auto curr = head;
				while (curr->next) {
					curr = curr->next;
				}
				curr->next = n;
			}
		});
	}

	uint64_t pop_back() {
		uint64_t ret = 0;

		transaction::run(plist_pop, [&] {
			if (head == nullptr)
				transaction::abort(EINVAL);

			auto curr = head;
			ret = head->value;
			while (curr->next) {
				ret = curr->next->value;
				if (curr->next->next == nullptr) {
					delete_persistent<pmem_entry>(curr->next);
					curr->next = nullptr;
					break;
				}
				curr = curr->next;
			}
		});

		return ret;
	}

	void show(void) const {
		for (auto n = head; n != nullptr; n = n->next)
			std::cout << n->value << std::endl;
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
                            if (!repeat(args, i, m,[&](vector<string> args){ return exec(args); } )) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    } else if (op == "push_back") {
						string n = next_op(args, i);
                        if (!n.empty()) {
                            uint64_t m = std::stoull(n);
                            push_back(m);
                        } else {
                            return false;
                        }
                    } else if (op == "push_front") {
						string n = next_op(args, i);
                        if (!n.empty()) {
                            uint64_t m = std::stoull(n);
                            push_front(m);
                        } else {
                            return false;
                        }
                    } else if (op == "pop_back") {
                        std::cout << pop_back() << std::endl;
                    } else if (op == "pop_front") {
                        std::cout << pop_front() << std::endl;
                    } else if (op == "run") {
						string filename = next_op(args, i);
                        if (!filename.empty()) {
                            return run(filename, [&](vector<string> args){ return exec(args); });
                        } else {
                            return false;
                        }
                    } else if (op == "print") {
                        show();
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
        		  << "  push_back data   Push data to the tail" << std::endl
        		  << "  push_front data  Push data to the head" << std::endl
        		  << "  pop_back         plist_pop an element from the tail" << std::endl
        		  << "  pop_front        plist_pop an element from the head" << std::endl
        		  << "  repeat n         Repeat the next operation n times" << std::endl
        		  << "  run file         Run a script file" << std::endl
        		  << "  clear            Delete all elements" << std::endl
        		  << "  print            Print the entire list" << std::endl
        		  << "  help             Display help" << std::endl;
	}

private:
	persistent_ptr<pmem_entry> head;
};

void perform_plist(vector<string> args) {
	const char *path = "list.pool";
	persistent_ptr<plist> self;

	try {
		if (file_exists(path) != 0) {
			plist_pop = pool<plist>::create(
				path, LIST_LAYOUT, PMEMOBJ_MIN_POOL, CREATE_MODE_RW);
		} else {
			plist_pop = pool<plist>::open(path, LIST_LAYOUT);
		}
		self = plist_pop.root();
	} catch (const pmem::pool_error &e) {
		std::cerr << "Exception: " << e.what() << std::endl;
		exit(1);
	} catch (const pmem::transaction_error &e) {
		std::cerr << "Exception: " << e.what() << std::endl;
		exit(1);
	}

	self->exec(args);

	try {
		plist_pop.close();
	} catch (const std::logic_error &e) {
		std::cerr << "Exception: " << e.what() << std::endl;
		exit(1);
	}
}