(function() {var implementors = {};
implementors["num_bigint"] = [{"text":"impl UpperHex for BigInt","synthetic":false,"types":[]},{"text":"impl UpperHex for BigUint","synthetic":false,"types":[]}];
implementors["num_complex"] = [{"text":"impl&lt;T&gt; UpperHex for Complex&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UpperHex + Num + PartialOrd + Clone,&nbsp;</span>","synthetic":false,"types":[]}];
implementors["num_rational"] = [{"text":"impl&lt;T:&nbsp;UpperHex + Clone + Integer&gt; UpperHex for Ratio&lt;T&gt;","synthetic":false,"types":[]}];
implementors["term_painter"] = [{"text":"impl&lt;T:&nbsp;UpperHex&gt; UpperHex for Painted&lt;T&gt;","synthetic":false,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()