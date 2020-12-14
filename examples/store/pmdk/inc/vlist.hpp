#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
#include <iterator>
#include <sys/stat.h>
#include "prog.hpp"

using namespace std;

class vlist;

class vlist {

	struct pmem_entry {
		pmem_entry *next;
		uint64_t value;
	};

public:
	vlist() { head = nullptr; }

	friend void perform_vlist(vector<string> args);

	void push_front(uint64_t value) {
		auto n = new pmem_entry;
		n->value = value;
		n->next = head;

		head = n;
	}

	uint64_t pop_front() {
		uint64_t ret = 0;
		if (head == nullptr)
			return 0;

		ret = head->value;
		auto n = head->next;

		delete head;
		head = n;

		return ret;
	}

	void push_back(uint64_t value) {
		auto n = new pmem_entry;

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
	}

	uint64_t pop_back() {
		if (head == nullptr)
			return 0;

		auto curr = head;
		auto ret = head->value;
		while (curr->next) {
			ret = curr->next->value;
			if (curr->next->next == nullptr) {
				delete curr->next;
				curr->next = nullptr;
				break;
			}
			curr = curr->next;
		}

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
        		  << "  pop_back         vlist_pop an element from the tail" << std::endl
        		  << "  pop_front        vlist_pop an element from the head" << std::endl
        		  << "  repeat n         Repeat the next operation n times" << std::endl
        		  << "  run file         Run a script file" << std::endl
        		  << "  clear            Delete all elements" << std::endl
        		  << "  print            Print the entire list" << std::endl
        		  << "  help             Display help" << std::endl;
	}

private:
	pmem_entry *head;
};

void perform_vlist(vector<string> args) {
	vlist self;
	self.exec(args);
}