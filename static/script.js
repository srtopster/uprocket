const AllVideos = document.querySelectorAll("video"); 
const AllImages = document.querySelectorAll("img");
let GlobalVolume = 1.0

//so carrega o video ou a imagem se estiver visivel
//o server agradece
let observer = new IntersectionObserver((entries, observer) => { 
    entries.forEach(entry => {
        if(entry.isIntersecting){
            if (entry.target.tagName == "IMG") {
                entry.target.src = entry.target.dataset.src
            }else if (entry.target.tagName == "VIDEO") {
                entry.target.preload = "auto"
            }
            observer.unobserve(entry.target);
        }
    });
}, {rootMargin: "0px 0px +100px 0px"});

AllVideos.forEach(video => {
    video.addEventListener("play", function() {
        video.volume = GlobalVolume
    })
    video.addEventListener("mouseover",function() {
        video.play()
    })
    video.addEventListener("mouseout",function() {
        video.pause()
    })
    video.addEventListener("volumechange",function() {
        GlobalVolume = video.volume
    })
    observer.observe(video);
});

AllImages.forEach(img => {
    observer.observe(img);
});
