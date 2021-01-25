(function() {var implementors = {};
implementors["corundum"] = [{"text":"impl Send for BuddyAlloc","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; Send for BuddyAlg&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: Send,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; Send for Zones&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: Send,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: Send,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Send for Heap","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; Send for PrcBox&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: Send,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: Send,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; Send for ParcInner&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: Send,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: Send,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Send for LogEnum","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; !Send for Notifier&lt;A&gt;","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; !Send for Log&lt;A&gt;","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; !Send for String&lt;A&gt;","synthetic":true,"types":[]},{"text":"impl&lt;T&gt; Send for IntoIteratorHelper&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: Send,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a, T&gt; Send for IterHelper&lt;'a, T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: Sync,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; Send for Measure&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: Send,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T&gt; Send for AssertTxInSafe&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: Send,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A:&nbsp;MemPool, T:&nbsp;?Sized&gt; !Send for Pbox&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + Send + ?Sized, A:&nbsp;MemPool&gt; Send for PCell&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + ?Sized, A:&nbsp;MemPool&gt; Send for PRefCell&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for Ref&lt;'_, T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for RefMut&lt;'_, T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + Send, A:&nbsp;MemPool&gt; Send for RootCell&lt;'_, T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;Default + VSafe + ?Sized, A:&nbsp;MemPool&gt; Send for VCell&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for Prc&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for Weak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for VWeak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;Send, A:&nbsp;MemPool&gt; Send for PMutex&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for MutexGuard&lt;'_, T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for Parc&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for Weak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSend + ?Sized, A:&nbsp;MemPool&gt; Send for VWeak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;A:&nbsp;MemPool, T&gt; !Send for FatPtr&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;A:&nbsp;MemPool, T&gt; !Send for Ptr&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized&gt; !Send for NonNull&lt;T&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; !Send for LogNonNull&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl Send for Chaperon","synthetic":false,"types":[]},{"text":"impl&lt;A:&nbsp;MemPool&gt; !Send for Journal&lt;A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T, A:&nbsp;MemPool&gt; !Send for Vec&lt;T, A&gt;","synthetic":false,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()